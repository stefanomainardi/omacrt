//! RGB-Pi 2 control over the HDMI DDC bus.
//!
//! The DAC is an I2C slave at 7 bit address 0x78. It powers up with separated
//! H and V sync, which a SCART TV cannot lock to, so the host selects
//! composite sync after every power cycle. Register map (page select is
//! register 0x00): page 4 reg 0xB5 = 0x06 AND, 0x0C XOR, 0x00 separated;
//! page 0 reg 0x60 = 0x00 asserts reset and 0xFF releases it; page 0 reg
//! 0x61 reads 0xFF while locked and 0xEF after a signal loss.

use std::ffi::CString;
use std::io;
use std::path::Path;
use std::time::Duration;

pub const ADDR: u16 = 0x78;
const I2C_SLAVE_FORCE: libc::c_ulong = 0x0706;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Csync {
    And,
    Xor,
    Separate,
}

impl Csync {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "and" => Some(Csync::And),
            "xor" => Some(Csync::Xor),
            "separate" | "separated" | "hv" => Some(Csync::Separate),
            _ => None,
        }
    }

    pub fn value(self) -> u8 {
        match self {
            Csync::And => 0x06,
            Csync::Xor => 0x0C,
            Csync::Separate => 0x00,
        }
    }

    pub fn from_value(v: u8) -> Option<Self> {
        match v {
            0x06 => Some(Csync::And),
            0x0C => Some(Csync::Xor),
            0x00 => Some(Csync::Separate),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Csync::And => "and",
            Csync::Xor => "xor",
            Csync::Separate => "separate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    Locked,
    Lost,
    Other(u8),
}

impl Lock {
    pub fn label(self) -> String {
        match self {
            Lock::Locked => "locked".into(),
            Lock::Lost => "lost".into(),
            Lock::Other(v) => format!("0x{v:02X}"),
        }
    }
}

pub struct Dac {
    fd: libc::c_int,
    pub bus: String,
}

impl Dac {
    /// Open the I2C bus device (`/dev/i2c-N`) and address the DAC.
    pub fn open(bus: &str) -> io::Result<Self> {
        let path = CString::new(bus).map_err(|_| io::Error::other("bad bus path"))?;
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let r = unsafe { libc::ioctl(fd, I2C_SLAVE_FORCE, ADDR as libc::c_ulong) };
        if r < 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(e);
        }
        Ok(Self {
            fd,
            bus: bus.to_string(),
        })
    }

    /// Bus device behind a DRM connector's `ddc` link.
    pub fn bus_of(connector: &Path) -> Option<String> {
        let ddc = std::fs::read_link(connector.join("ddc")).ok()?;
        let name = ddc.file_name()?.to_str()?.to_string();
        Some(format!("/dev/{name}"))
    }

    fn write(&self, reg: u8, value: u8) -> io::Result<()> {
        let buf = [reg, value];
        let n = unsafe { libc::write(self.fd, buf.as_ptr() as *const libc::c_void, 2) };
        if n != 2 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn read(&self, reg: u8) -> io::Result<u8> {
        let n = unsafe { libc::write(self.fd, [reg].as_ptr() as *const libc::c_void, 1) };
        if n != 1 {
            return Err(io::Error::last_os_error());
        }
        let mut out = [0u8; 1];
        let n = unsafe { libc::read(self.fd, out.as_mut_ptr() as *mut libc::c_void, 1) };
        if n != 1 {
            return Err(io::Error::last_os_error());
        }
        Ok(out[0])
    }

    fn page(&self, n: u8) -> io::Result<()> {
        self.write(0x00, n)
    }

    /// True when something acknowledges at 0x78.
    pub fn present(&self) -> bool {
        self.page(0).is_ok()
    }

    pub fn set_csync(&self, mode: Csync) -> io::Result<()> {
        self.page(4)?;
        self.write(0xB5, mode.value())
    }

    pub fn csync(&self) -> io::Result<u8> {
        self.page(4)?;
        self.read(0xB5)
    }

    /// Lock state. RePlayOS only acts on the 0xFF to 0xEF transition; other
    /// bits of the register flicker during mode changes, so anything but
    /// 0xEF counts as locked.
    pub fn lock(&self) -> io::Result<Lock> {
        Ok(match self.lock_raw()? {
            0xEF => Lock::Lost,
            _ => Lock::Locked,
        })
    }

    pub fn lock_raw(&self) -> io::Result<u8> {
        self.page(0)?;
        self.read(0x61)
    }

    /// Two second reset pulse. The reset clears the csync selection, so the
    /// previous value is written back afterwards.
    pub fn reset(&self) -> io::Result<()> {
        let keep = self.csync().unwrap_or(0);
        self.page(0)?;
        self.write(0x60, 0x00)?;
        std::thread::sleep(Duration::from_millis(2000));
        self.write(0x60, 0xFF)?;
        std::thread::sleep(Duration::from_millis(200));
        self.page(4)?;
        self.write(0xB5, keep)
    }

    /// Poll the lock register and reset the DAC when the signal drops, the
    /// way RePlayOS works around the PLL issue of this hardware revision.
    pub fn watch(&self, mut on_event: impl FnMut(&str)) -> io::Result<()> {
        let mut last = self.lock()?;
        on_event(&format!("watching {}, {}", self.bus, last.label()));
        loop {
            std::thread::sleep(Duration::from_millis(1));
            let Ok(now) = self.lock() else { continue };
            if last != Lock::Lost && now == Lock::Lost {
                on_event("lock lost, resetting");
                self.reset()?;
            }
            last = now;
        }
    }
}

impl Drop for Dac {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}
