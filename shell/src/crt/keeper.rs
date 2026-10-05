//! The lease keeper: the one process the tube cannot do without.
//!
//! Hyprland lends the television's connector through `wp_drm_lease_v1`, and
//! two facts about that loan decide the shape of this module. It is revoked
//! the moment the client that took it disconnects: the protocol object goes,
//! and Aquamarine's lease destructor calls `drmModeRevokeLease`. And a
//! connector that has been offered once is never offered again in the same
//! session (`m_connectorsSent` in Hyprland's `DRMLease.cpp`). So whatever
//! process holds the lease must never end, or the tube is gone until the next
//! login.
//!
//! The display process is four and a half thousand lines of compositor, GPU
//! and clients, and it is the part that changes and the part that can crash.
//! The keeper is the rest: it takes the lease when the tube is first turned
//! on, keeps the Wayland connection that the lease lives on, and hands the
//! lease's DRM descriptor to one display process at a time over a Unix
//! socket. In the kernel the lease's master belongs to the open file, not to
//! a process, so the descriptor passed with `SCM_RIGHTS` carries everything,
//! and the lease outlives any display process for as long as the keeper's own
//! copy is open. A display process can then be restarted, after a crash by
//! the watchdog or for an update, and take the tube back.
//!
//! The protocol is a line of text each way, with the descriptor riding on the
//! keeper's answer to `take`:
//!
//! ```text
//! take HDMI-A-1   ->  ok 101 HDMI-A-1      (and the descriptor)
//!                 ->  err <why>
//! status          ->  ok HDMI-A-1 101 holder 4242 | holder none
//! release         ->  ok                   (the keeper exits, the lease ends)
//! ```
//!
//! A display process keeps its connection open for as long as it drives the
//! tube: that is how the keeper knows the tube is taken, and how the display
//! process learns the lease has been revoked (the keeper writes `revoked`
//! and closes).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The keeper's process and file name.
pub const BINARY: &str = "flyback-keeper";

/// Where the keeper listens: the session's runtime folder, which goes away
/// with the session exactly as the lease does.
pub fn socket_path() -> PathBuf {
    runtime_dir().join("keeper.sock")
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(super::state_dir)
        .join("omacrt")
}

pub fn log_path() -> PathBuf {
    super::state_dir().join("keeper.log")
}

/// The keeper's binary, found the way the display process's is.
pub fn binary() -> PathBuf {
    super::display::sibling(BINARY)
}

/// What a display process gets back from the keeper.
pub struct Taken {
    /// The lease's DRM descriptor, master for the connector, its CRTC and
    /// planes, until the display process takes it to drive the tube with.
    pub fd: Option<OwnedFd>,
    pub connector_id: u32,
    pub name: String,
    /// Held open for as long as the tube is driven.
    stream: UnixStream,
}

impl Taken {
    /// Whether the keeper still has the lease. A `revoked` line or the
    /// connection closing both mean it does not.
    pub fn alive(&mut self) -> bool {
        let _ = self.stream.set_nonblocking(true);
        let mut buf = [0u8; 64];
        match self.stream.read(&mut buf) {
            Ok(0) => false,
            Ok(n) => !String::from_utf8_lossy(&buf[..n]).contains("revoked"),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => true,
            Err(_) => false,
        }
    }
}

/// Is a keeper answering on the socket?
pub fn running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

/// Ask the keeper for the lease on `want`.
pub fn take(want: &str) -> Result<Taken, String> {
    let mut stream = UnixStream::connect(socket_path()).map_err(|e| format!("keeper: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "take {want}").map_err(|e| format!("keeper: {e}"))?;
    let (line, fd) = recv_with_fd(&stream).map_err(|e| format!("keeper: {e}"))?;
    let reply = parse_reply(&line)?;
    let mut words = reply.split_whitespace();
    let connector_id = words
        .next()
        .and_then(|w| w.parse().ok())
        .ok_or_else(|| format!("keeper: a reply without a connector: {line}"))?;
    let name = words.next().unwrap_or(want).to_string();
    let fd = fd.ok_or("keeper: the reply came without the descriptor")?;
    let _ = stream.set_read_timeout(None);
    Ok(Taken {
        fd: Some(fd),
        connector_id,
        name,
        stream,
    })
}

/// One line to the keeper and its answer.
fn ask(line: &str) -> Result<String, String> {
    let mut stream = UnixStream::connect(socket_path()).map_err(|e| format!("keeper: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "{line}").map_err(|e| format!("keeper: {e}"))?;
    let mut reply = String::new();
    BufReader::new(&stream)
        .read_line(&mut reply)
        .map_err(|e| format!("keeper: {e}"))?;
    parse_reply(reply.trim())
}

/// The keeper's own account of what it holds.
pub fn status() -> Result<String, String> {
    ask("status")
}

/// End the lease: the keeper exits and the desktop gets the connector back,
/// for the rest of the session. Refused while a display process drives it.
pub fn release() -> Result<String, String> {
    ask("release")
}

/// `ok ...` is the rest of the line; `err ...` is an error in those words.
pub fn parse_reply(line: &str) -> Result<String, String> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("ok") {
        Ok(rest.trim().to_string())
    } else if let Some(rest) = line.strip_prefix("err") {
        Err(rest.trim().to_string())
    } else {
        Err(format!("keeper: an answer it does not speak: {line}"))
    }
}

/// Start a keeper for `connector`, unless one is already answering, and wait
/// until it listens. The keeper takes the lease before it listens, so a
/// socket that answers is a lease that is held.
pub fn start(connector: &str) -> Result<String, String> {
    if running() {
        return Ok("already holding".into());
    }
    let _ = std::fs::create_dir_all(super::state_dir());
    let log = crate::logfile::open(&log_path()).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let bin = binary();
    let mut cmd = Command::new(&bin);
    cmd.arg(connector)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", bin.display()))?;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(6) {
        if running() {
            return Ok("holding".into());
        }
        if let Ok(Some(status)) = child.try_wait() {
            let tail = std::fs::read_to_string(log_path())
                .ok()
                .and_then(|t| t.lines().last().map(str::to_string))
                .unwrap_or_default();
            return Err(format!("the keeper exited ({status}): {tail}"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("the keeper did not come up".into())
}

/// The keeper's side of a socket: bound in a folder only this user can enter,
/// and the socket itself readable by this user alone.
pub fn bind() -> std::io::Result<std::os::unix::net::UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let path = socket_path();
    // A socket file nobody answers on is what a keeper that died leaves.
    if path.exists() && UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let listener = std::os::unix::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// The user and process on the other end of a connection.
pub fn peer(stream: &UnixStream) -> Option<(u32, i32)> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    (r == 0).then_some((cred.uid, cred.pid))
}

/// Write one line with a descriptor riding on it.
pub fn send_with_fd(stream: &UnixStream, line: &str, fd: RawFd) -> std::io::Result<()> {
    let mut text = line.as_bytes().to_vec();
    text.push(b'\n');
    let mut iov = libc::iovec {
        iov_base: text.as_mut_ptr() as *mut libc::c_void,
        iov_len: text.len(),
    };
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = space as _;
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(cmsg) as *mut RawFd, fd);
    }
    let sent = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) };
    if sent < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if sent as usize != text.len() {
        return Err(std::io::Error::other("short write"));
    }
    Ok(())
}

/// Read one line, and the descriptor if one came with it. The line is read
/// in one message: the keeper's answers are short and sent whole.
pub fn recv_with_fd(stream: &UnixStream) -> std::io::Result<(String, Option<OwnedFd>)> {
    let mut buf = vec![0u8; 512];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr() as *mut libc::c_void,
        iov_len: buf.len(),
    };
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = space as _;
    let n = unsafe { libc::recvmsg(stream.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if n == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "closed",
        ));
    }
    let mut fd = None;
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let raw = std::ptr::read_unaligned(libc::CMSG_DATA(cmsg) as *const RawFd);
                fd = Some(OwnedFd::from_raw_fd(raw));
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    let line = String::from_utf8_lossy(&buf[..n as usize])
        .trim()
        .to_string();
    Ok((line, fd))
}

/// The display process holding the tube: its connection and its pid.
struct Holder {
    stream: UnixStream,
    pid: i32,
}

impl Holder {
    /// Whether the other end is still there. A display process that exits,
    /// cleanly or not, closes its end: the kernel does that for it.
    fn alive(&mut self) -> bool {
        let _ = self.stream.set_nonblocking(true);
        let mut buf = [0u8; 64];
        match self.stream.read(&mut buf) {
            Ok(0) => false,
            Ok(_) => true,
            Err(e) => e.kind() == std::io::ErrorKind::WouldBlock,
        }
    }
}

/// The keeper's state, apart from the lease and the loop: what it holds and
/// who has it. Kept here, away from the compositor, so it can be tested.
pub struct Server {
    fd: OwnedFd,
    name: String,
    connector_id: u32,
    holder: Option<Holder>,
    uid: u32,
}

/// What a request asks the main loop to do afterwards.
pub enum After {
    Carry,
    Release,
}

impl Server {
    pub fn new(fd: OwnedFd, name: String, connector_id: u32) -> Server {
        Server {
            fd,
            name,
            connector_id,
            holder: None,
            uid: unsafe { libc::getuid() },
        }
    }

    /// The holder's socket, for the loop to wait on.
    pub fn holder_fd(&self) -> Option<RawFd> {
        self.holder.as_ref().map(|h| h.stream.as_raw_fd())
    }

    /// The holder's socket said something or hung up: forget a holder that
    /// has gone.
    pub fn notice_hangup(&mut self) {
        if let Some(h) = &mut self.holder
            && !h.alive()
        {
            println!("display process {} let go of the tube", h.pid);
            self.holder = None;
        }
    }

    /// The lease has ended: tell the display process driving the tube.
    pub fn revoked(&self) {
        if let Some(h) = &self.holder {
            let _ = (&h.stream).write_all(b"revoked\n");
        }
    }

    pub fn handle(&mut self, stream: UnixStream) -> After {
        let Some((uid, pid)) = peer(&stream) else {
            return After::Carry;
        };
        if uid != self.uid {
            eprintln!("refused a connection from uid {uid}");
            return After::Carry;
        }
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            return After::Carry;
        }
        let line = line.trim();
        let reply = |text: &str| {
            let _ = (&stream).write_all(format!("{text}\n").as_bytes());
        };
        // A holder whose process has gone is no holder: the restart that
        // follows a crash asks before this loop has noticed the hang-up.
        if let Some(h) = &mut self.holder
            && !h.alive()
        {
            println!("display process {} let go of the tube", h.pid);
            self.holder = None;
        }
        match line.split_once(' ').unwrap_or((line, "")) {
            ("take", want) => {
                let want = want.trim();
                if want != self.name {
                    reply(&format!("err this keeper holds {}, not {want}", self.name));
                    return After::Carry;
                }
                if let Some(h) = &self.holder {
                    reply(&format!(
                        "err busy: display process {} drives the tube",
                        h.pid
                    ));
                    return After::Carry;
                }
                let ok = format!("ok {} {}", self.connector_id, self.name);
                match send_with_fd(&stream, &ok, self.fd.as_raw_fd()) {
                    Ok(()) => {
                        println!("handed the tube to display process {pid}");
                        self.holder = Some(Holder { stream, pid });
                    }
                    Err(e) => eprintln!("could not hand the lease to {pid}: {e}"),
                }
                After::Carry
            }
            ("status", _) => {
                let holder = self
                    .holder
                    .as_ref()
                    .map(|h| h.pid.to_string())
                    .unwrap_or_else(|| "none".into());
                reply(&format!(
                    "ok {} {} holder {holder}",
                    self.name, self.connector_id
                ));
                After::Carry
            }
            ("release", _) => {
                if let Some(h) = &self.holder {
                    reply(&format!(
                        "err busy: display process {} drives the tube",
                        h.pid
                    ));
                    return After::Carry;
                }
                reply("ok");
                After::Release
            }
            _ => {
                reply(&format!("err not a request: {line}"));
                After::Carry
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_descriptor_crosses_the_socket_and_still_works() {
        let (a, b) = UnixStream::pair().unwrap();
        // A pipe stands in for the DRM descriptor: what crosses is the open
        // file, so writing to the copy that arrived reaches the read end.
        let (mut r, w) = {
            let mut fds = [0; 2];
            assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
            unsafe {
                (
                    std::fs::File::from_raw_fd(fds[0]),
                    OwnedFd::from_raw_fd(fds[1]),
                )
            }
        };
        send_with_fd(&a, "ok 101 HDMI-A-1", w.as_raw_fd()).unwrap();
        drop(w);
        let (line, fd) = recv_with_fd(&b).unwrap();
        assert_eq!(line, "ok 101 HDMI-A-1");
        let mut copy = std::fs::File::from(fd.expect("a descriptor"));
        copy.write_all(b"tube").unwrap();
        drop(copy);
        let mut got = String::new();
        r.read_to_string(&mut got).unwrap();
        assert_eq!(got, "tube");
    }

    #[test]
    fn a_line_without_a_descriptor_reads_as_one() {
        let (mut a, b) = UnixStream::pair().unwrap();
        a.write_all(b"err busy\n").unwrap();
        let (line, fd) = recv_with_fd(&b).unwrap();
        assert_eq!(line, "err busy");
        assert!(fd.is_none());
    }

    #[test]
    fn replies_read_as_values_and_errors() {
        assert_eq!(parse_reply("ok 101 HDMI-A-1"), Ok("101 HDMI-A-1".into()));
        assert_eq!(
            parse_reply("err busy: held by 42"),
            Err("busy: held by 42".into())
        );
        assert!(parse_reply("hello").is_err());
    }

    #[test]
    fn the_other_end_of_a_pair_is_this_user() {
        let (a, _b) = UnixStream::pair().unwrap();
        let (uid, pid) = peer(&a).expect("credentials");
        assert_eq!(uid, unsafe { libc::getuid() });
        assert_eq!(pid, std::process::id() as i32);
    }

    #[test]
    fn a_revoked_line_or_a_closed_socket_ends_the_lease() {
        let (mut k, b) = UnixStream::pair().unwrap();
        let fd = OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
        let mut t = Taken {
            fd: Some(fd),
            connector_id: 1,
            name: "X".into(),
            stream: b,
        };
        assert!(t.alive(), "nothing said yet");
        k.write_all(b"revoked\n").unwrap();
        assert!(!t.alive());
        let (k2, b2) = UnixStream::pair().unwrap();
        let fd = OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
        let mut t = Taken {
            fd: Some(fd),
            connector_id: 1,
            name: "X".into(),
            stream: b2,
        };
        drop(k2);
        assert!(!t.alive());
    }

    /// A server holding a pipe as its "lease", and a client's request.
    fn server() -> (Server, std::fs::File) {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let (r, w) = unsafe {
            (
                std::fs::File::from_raw_fd(fds[0]),
                OwnedFd::from_raw_fd(fds[1]),
            )
        };
        (Server::new(w, "HDMI-A-1".into(), 101), r)
    }

    fn request(s: &mut Server, line: &str) -> (UnixStream, String, Option<OwnedFd>, bool) {
        let (mut c, k) = UnixStream::pair().unwrap();
        c.write_all(format!("{line}\n").as_bytes()).unwrap();
        let release = matches!(s.handle(k), After::Release);
        let (reply, fd) = recv_with_fd(&c).unwrap_or_default();
        (c, reply, fd, release)
    }

    #[test]
    fn the_tube_is_lent_to_one_display_process_at_a_time() {
        let (mut s, mut lease_end) = server();
        let (first, reply, fd, _) = request(&mut s, "take HDMI-A-1");
        assert_eq!(reply, "ok 101 HDMI-A-1");
        // What arrived is the keeper's own open file.
        let mut copy = std::fs::File::from(fd.expect("the descriptor"));
        copy.write_all(b"x").unwrap();
        let mut b = [0u8; 1];
        lease_end.read_exact(&mut b).unwrap();
        let (_c, reply, fd, _) = request(&mut s, "take HDMI-A-1");
        assert!(reply.starts_with("err busy"), "{reply}");
        assert!(fd.is_none());
        // The first display process dies: the next one gets the tube.
        drop(first);
        let (_c, reply, fd, _) = request(&mut s, "take HDMI-A-1");
        assert_eq!(reply, "ok 101 HDMI-A-1");
        assert!(fd.is_some());
    }

    #[test]
    fn a_connector_the_keeper_does_not_hold_is_refused() {
        let (mut s, _r) = server();
        let (_c, reply, fd, _) = request(&mut s, "take DP-1");
        assert!(
            reply.starts_with("err this keeper holds HDMI-A-1"),
            "{reply}"
        );
        assert!(fd.is_none());
    }

    #[test]
    fn status_names_the_holder_and_release_waits_for_none() {
        let (mut s, _r) = server();
        let (_c, reply, _, _) = request(&mut s, "status");
        assert_eq!(reply, "ok HDMI-A-1 101 holder none");
        let (holder, _, _, _) = request(&mut s, "take HDMI-A-1");
        let (_c, reply, _, _) = request(&mut s, "status");
        assert_eq!(
            reply,
            format!("ok HDMI-A-1 101 holder {}", std::process::id())
        );
        let (_c, reply, _, release) = request(&mut s, "release");
        assert!(reply.starts_with("err busy") && !release, "{reply}");
        drop(holder);
        let (_c, reply, _, release) = request(&mut s, "release");
        assert_eq!(reply, "ok");
        assert!(release);
    }

    #[test]
    fn a_revocation_reaches_the_display_process() {
        let (mut s, _r) = server();
        let (c, _, fd, _) = request(&mut s, "take HDMI-A-1");
        let mut t = Taken {
            fd,
            connector_id: 101,
            name: "HDMI-A-1".into(),
            stream: c,
        };
        assert!(t.alive());
        s.revoked();
        assert!(!t.alive());
    }
}
