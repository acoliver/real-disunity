//! Scoped callbacks into the host's file reader. Foreign buffers are borrowed only during a call.
use std::ffi::{OsStr, c_void};
use std::sync::Mutex;

use crate::file_reads::{AdmissionError, FileReads, ReadLimits, ReadStatus};
use crate::{HostError, ManagedHostConfig};
use netcorehost::pdcstring::PdCString;

// Operations: submit=0, status=1, count=2, error=3, data=4, cancel=5,
// release=6, complete=7, valid=8. Negative results: stale=-1, argument=-2,
// capacity=-3, pending=-4. Submit uses id as offset and value as size.
type Callback = unsafe extern "C" fn(*mut c_void, u32, u64, *mut u8, u32, u64) -> i64;
type Configure = unsafe extern "system" fn(*mut c_void, Option<Callback>) -> i32;

pub(crate) struct FileReadBridge {
    context: Box<Mutex<FileReads>>,
    configure: Configure,
}

impl FileReadBridge {
    pub fn attach(config: &ManagedHostConfig) -> Result<Self, HostError> {
        let loader = crate::managed_entrypoints::managed_loader(config)?;
        let name = |text: &str| {
            PdCString::from_os_str(OsStr::new(text))
                .map_err(|error| HostError::Hostfxr(error.to_string()))
        };
        let configure = *loader
            .get_function_with_unmanaged_callers_only::<Configure>(
                &name("Disunity.Hosting.AsyncReadBridge, Disunity.Engine")?,
                &name("Configure")?,
            )
            .map_err(|error| HostError::Hostfxr(error.to_string()))?;
        let mut bridge = Self {
            context: Box::new(Mutex::new(
                FileReads::new(ReadLimits::default()).map_err(HostError::FileReads)?,
            )),
            configure,
        };
        // SAFETY: the box stays at a stable address until Configure detaches and drains callers.
        let code = unsafe {
            configure(
                (&mut *bridge.context as *mut Mutex<FileReads>).cast(),
                Some(call),
            )
        };
        if code != 0 {
            return Err(HostError::Managed("cannot attach native file reads".into()));
        }
        Ok(bridge)
    }
}

impl Drop for FileReadBridge {
    fn drop(&mut self) {
        // SAFETY: managed detachment excludes new calls and waits for borrowed calls to return.
        assert_eq!(unsafe { (self.configure)(std::ptr::null_mut(), None) }, 0);
    }
}

unsafe extern "C" fn call(
    context: *mut c_void,
    operation: u32,
    id: u64,
    buffer: *mut u8,
    length: u32,
    value: u64,
) -> i64 {
    if context.is_null() || (buffer.is_null() && length != 0) {
        return -2;
    }
    // SAFETY: the owning bridge lends its stable context until managed detachment drains callers.
    let reader = unsafe { &*context.cast::<Mutex<FileReads>>() };
    if operation == 0 {
        if length == 0 || length > 4096 || value > u64::from(u32::MAX) {
            return -2;
        }
        // SAFETY: submit borrows exactly length UTF-8 path bytes for this call only.
        let bytes = unsafe { std::slice::from_raw_parts(buffer, length as usize) };
        let Ok(path) = std::str::from_utf8(bytes) else {
            return -2;
        };
        if path.contains('\0') {
            return -2;
        }
        return match reader.lock().expect("file reader panicked").submit(
            path.into(),
            id,
            value as usize,
        ) {
            Ok(handle) => handle as i64,
            Err(AdmissionError::InvalidRange) => -2,
            Err(AdmissionError::Capacity) => -3,
            Err(AdmissionError::Closed) => -1,
        };
    }
    if operation == 6 {
        return if reader.lock().expect("file reader panicked").release(id) {
            0
        } else {
            -1
        };
    }
    let Some(request) = reader.lock().expect("file reader panicked").request(id) else {
        return -1;
    };
    match operation {
        1 => request.status() as i64,
        2 => request.bytes_read() as i64,
        3 => {
            let error = request.error().unwrap_or_default();
            // SAFETY: copy borrows the caller's output only for the duration of this invocation.
            unsafe { copy(error.as_bytes(), buffer, length) }
        }
        4 => {
            if request.status() == ReadStatus::InProgress {
                return -4;
            }
            let size = request.bytes_read();
            if (length as usize) < size {
                return size as i64;
            }
            let mut empty = [];
            let output = if length == 0 {
                &mut empty[..]
            } else {
                // SAFETY: output is writable for length bytes and is never retained.
                unsafe { std::slice::from_raw_parts_mut(buffer, length as usize) }
            };
            request.copy_data(output).map_or(-2, |count| count as i64)
        }
        5 => {
            request.cancel();
            0
        }
        7 => {
            request.complete();
            0
        }
        8 => 1,
        _ => -2,
    }
}

unsafe fn copy(bytes: &[u8], output: *mut u8, capacity: u32) -> i64 {
    if bytes.len() <= capacity as usize && !bytes.is_empty() {
        // SAFETY: caller validated capacity bytes of writable storage; no overlapping engine buffer.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len()) };
    }
    bytes.len() as i64
}

#[cfg(test)]
#[path = "file_reads_managed_tests.rs"]
mod tests;
