//! Safe kernel monotonic input, independent of managed-host initialization.

use std::{io, time::Duration};

/// A kernel clock reader with a fixed OS-owned origin and immutable conversion metadata.
///
/// Opening or sampling does not boot a managed host or alter engine state. Equal samples
/// and zero are valid. Suspend behavior and resolution are platform-specific.
#[derive(Debug)]
pub struct KernelMonotonic {
    #[cfg(target_os = "macos")]
    timebase: Timebase,
}

impl KernelMonotonic {
    /// Opens the platform source without subtracting a startup sample.
    ///
    /// Fails with the OS cause, `InvalidData` for malformed metadata, or `Unsupported`
    /// on platforms other than Linux, macOS and Windows.
    pub fn open() -> io::Result<Self> {
        #[cfg(target_os = "macos")]
        {
            let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
            // SAFETY: the OS writes only to this initialized, valid timebase structure.
            let status = unsafe { mach_timebase_info(&raw mut info) };
            mach_status(status)?;
            Ok(Self {
                timebase: Timebase::new(info.numer, info.denom)?,
            })
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            Ok(Self {})
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Err(unsupported())
        }
    }

    /// Reads one absolute sample. No last-good value, elapsed clock or fallback is used.
    ///
    /// Linux preserves the OS error from `clock_gettime`; invalid fields and conversion
    /// overflow return `InvalidData`. macOS and Windows raw reads have no failure sentinel.
    pub fn sample(&self) -> io::Result<Duration> {
        #[cfg(target_os = "linux")]
        {
            let mut value = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            // SAFETY: CLOCK_MONOTONIC is a supported clock and value is a valid out-pointer.
            let status = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut value) };
            unix_status(status, io::Error::last_os_error)?;
            unix_duration(i128::from(value.tv_sec), i128::from(value.tv_nsec))
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: mach_absolute_time takes no pointers and has no failure sentinel.
            self.timebase.duration(unsafe { mach_absolute_time() })
        }
        #[cfg(target_os = "windows")]
        {
            // SAFETY: GetTickCount64 takes no pointers and is available on supported Windows.
            Ok(Duration::from_millis(unsafe { GetTickCount64() }))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Err(unsupported())
        }
    }
}

// libc deprecates its Mach declarations; keep the small system ABI here without
// adding a second platform dependency or suppressing deprecation diagnostics.
#[cfg(target_os = "macos")]
#[repr(C)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> libc::c_int;
    fn mach_absolute_time() -> u64;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount64() -> u64;
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(any(target_os = "linux", test))]
fn unix_status(status: i32, error: impl FnOnce() -> io::Error) -> io::Result<()> {
    if status == 0 { Ok(()) } else { Err(error()) }
}

#[cfg(any(target_os = "linux", test))]
fn unix_duration(seconds: i128, nanos: i128) -> io::Result<Duration> {
    let seconds = u64::try_from(seconds)
        .map_err(|_| invalid("CLOCK_MONOTONIC seconds are negative or exceed Duration"))?;
    if !(0..1_000_000_000).contains(&nanos) {
        return Err(invalid("CLOCK_MONOTONIC nanoseconds are out of range"));
    }
    let nanos =
        u32::try_from(nanos).map_err(|_| invalid("CLOCK_MONOTONIC nanoseconds exceed Duration"))?;
    Ok(Duration::new(seconds, nanos))
}

#[cfg(any(target_os = "macos", test))]
fn mach_status(status: i32) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "mach_timebase_info failed with Mach status {status}"
        )))
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug)]
struct Timebase {
    numer: u32,
    denom: u32,
}

#[cfg(any(target_os = "macos", test))]
impl Timebase {
    fn new(numer: u32, denom: u32) -> io::Result<Self> {
        if numer == 0 || denom == 0 {
            return Err(invalid("mach timebase ratio must be positive"));
        }
        Ok(Self { numer, denom })
    }

    fn duration(&self, ticks: u64) -> io::Result<Duration> {
        // A u64 sample times a u32 numerator fits in u128 without losing raw tick precision.
        let nanos = u128::from(ticks) * u128::from(self.numer) / u128::from(self.denom);
        duration_from_nanos(nanos)
    }
}

#[cfg(any(target_os = "macos", test))]
fn duration_from_nanos(nanos: u128) -> io::Result<Duration> {
    let seconds = u64::try_from(nanos / 1_000_000_000)
        .map_err(|_| invalid("mach monotonic sample exceeds Duration"))?;
    let subsec = u32::try_from(nanos % 1_000_000_000)
        .map_err(|_| invalid("mach monotonic nanoseconds exceed Duration"))?;
    Ok(Duration::new(seconds, subsec))
}

#[cfg(any(
    test,
    not(any(target_os = "linux", target_os = "macos", target_os = "windows"))
))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "kernel monotonic input is unsupported on this target",
    )
}

#[cfg(test)]
mod tests;
