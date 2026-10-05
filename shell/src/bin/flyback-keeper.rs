//! flyback-keeper: holds the television's DRM lease for the whole session.
//!
//! Hyprland revokes a lease when the client that took it disconnects, and
//! never offers that connector again until the next login. This process is
//! the client that took it. It does nothing else: no GPU, no buffers, no
//! Wayland clients of its own. It keeps the connection the lease lives on
//! serviced, and hands the lease's descriptor to one display process at a
//! time, so the display process can crash, be restarted by the watchdog or be
//! replaced by an update without the tube going dark for the session.
//!
//! The protocol and the client side are in `omacrt_shell::crt::keeper`.
//!
//! It ends in three ways only: the compositor revokes the lease, the
//! compositor goes away, or somebody asks for `release` while no display
//! process holds the tube.

#[allow(dead_code)]
#[path = "flyback/lease.rs"]
mod lease;

use std::os::fd::AsRawFd;

use omacrt_shell::crt::keeper::{self, After, Server};

fn main() {
    let Some(want) = std::env::args().nth(1) else {
        eprintln!("usage: flyback-keeper <connector>   (normally started by `omacrt on`)");
        std::process::exit(2);
    };
    if keeper::running() {
        println!("a keeper already holds the tube");
        return;
    }
    let mut lease = match lease::Lease::take(&want) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let Some(fd) = lease.fd.take() else {
        eprintln!("the lease arrived without a file descriptor");
        std::process::exit(1);
    };
    // Listening is what tells `omacrt on` the lease is held, so it comes
    // after the lease and never before.
    let listener = match keeper::bind() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{}: {e}", keeper::socket_path().display());
            std::process::exit(1);
        }
    };
    let _ = listener.set_nonblocking(true);
    println!(
        "holding {} (DRM connector {})",
        lease.name, lease.connector_id
    );
    let mut k = Server::new(fd, lease.name.clone(), lease.connector_id);
    let why = loop {
        let mut fds = vec![
            libc::pollfd {
                fd: lease.poll_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if let Some(fd) = k.holder_fd() {
            fds.push(libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
        }
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 1000) };
        if !lease.pump() {
            break "the compositor revoked the lease";
        }
        if fds.len() > 2 && fds[2].revents != 0 {
            k.notice_hangup();
        }
        let mut release = false;
        while let Ok((stream, _)) = listener.accept() {
            if let After::Release = k.handle(stream) {
                release = true;
            }
        }
        if release {
            break "released on request: the desktop has the connector back";
        }
    };
    println!("{why}");
    k.revoked();
    let _ = std::fs::remove_file(keeper::socket_path());
}
