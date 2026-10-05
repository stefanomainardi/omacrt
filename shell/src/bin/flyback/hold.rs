//! Where the display process gets its lease from.
//!
//! From the keeper when one is running, which is how `omacrt on` starts the
//! tube: the lease then outlives this process, and a display process started
//! after a crash or an update takes the tube back. Straight from the
//! compositor otherwise, as a display process started by hand for a
//! measurement does; that lease ends with this process, as it always did.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use omacrt_shell::crt::keeper;

use crate::lease::Lease;

pub struct Hold {
    pub name: String,
    pub connector_id: u32,
    from: From,
}

enum From {
    Compositor(Lease),
    Keeper(keeper::Taken),
}

impl Hold {
    /// The lease on `want` and its DRM descriptor.
    pub fn take(want: &str) -> Result<(Hold, OwnedFd), String> {
        if keeper::running() {
            let mut taken = keeper::take(want)?;
            let fd = taken.fd.take().ok_or("the keeper sent no descriptor")?;
            // A display process before this one may have died with buffers
            // on the descriptor. They belong to the open file the keeper
            // shares, not to the process that made them, so nothing freed
            // them; this process owns nothing yet and can sweep them all.
            let (fbs, handles) = sweep(fd.as_fd());
            if fbs + handles > 0 {
                println!(
                    "cleared what a previous display process left: {fbs} framebuffer(s), {handles} buffer handle(s)"
                );
            }
            let hold = Hold {
                name: taken.name.clone(),
                connector_id: taken.connector_id,
                from: From::Keeper(taken),
            };
            return Ok((hold, fd));
        }
        let mut lease = Lease::take(want)?;
        let fd = lease
            .fd
            .take()
            .ok_or("the lease arrived without a file descriptor")?;
        let hold = Hold {
            name: lease.name.clone(),
            connector_id: lease.connector_id,
            from: From::Compositor(lease),
        };
        Ok((hold, fd))
    }

    /// Whether the lease is still ours.
    pub fn pump(&mut self) -> bool {
        match &mut self.from {
            From::Compositor(l) => l.pump(),
            From::Keeper(t) => t.alive(),
        }
    }

    /// Whether the lease outlives this process.
    pub fn kept(&self) -> bool {
        matches!(self.from, From::Keeper(_))
    }
}

struct Raw<'a>(BorrowedFd<'a>);
impl AsFd for Raw<'_> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0
    }
}
impl drm::Device for Raw<'_> {}
impl drm::control::Device for Raw<'_> {}

/// Remove every framebuffer and close every buffer handle on the descriptor.
///
/// Only right before this process has made any of its own. Framebuffers are
/// listed by the kernel for the open file; buffer handles are not listed, but
/// the kernel gives them out lowest first from one, so walking up from one
/// until a long run of numbers in a row are free finds them all.
fn sweep(fd: BorrowedFd<'_>) -> (usize, usize) {
    use drm::control::Device as _;
    let dev = Raw(fd);
    let mut fbs = 0;
    if let Ok(res) = dev.resource_handles() {
        for fb in res.framebuffers() {
            if dev.destroy_framebuffer(*fb).is_ok() {
                fbs += 1;
            }
        }
    }
    let mut handles = 0;
    let mut free_in_a_row = 0;
    for h in 1u32..65536 {
        if free_in_a_row >= 1024 {
            break;
        }
        let Some(raw) = std::num::NonZeroU32::new(h) else {
            continue;
        };
        if dev.close_buffer(drm::buffer::Handle::from(raw)).is_ok() {
            handles += 1;
            free_in_a_row = 0;
        } else {
            free_in_a_row += 1;
        }
    }
    (fbs, handles)
}
