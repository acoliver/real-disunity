//! The synchronous GPU flush: a host registers a callback that runs every piece of pending
//! offscreen work, dispatches included, and the managed engine calls the flush when a script
//! reads a result back.
//!
//! The callback runs with no world lock or borrow held. It can call the ABI on the world being
//! flushed: take the pending work, read what a dispatch ran with and write the results into
//! compute buffers. A flush started from inside the callback is refused.
use crate::{DisunityResult, abi::translate_error, lock_store};

/// The host's flush: called with the world and the `user` word it was registered with, it fills
/// `message` (`capacity` bytes) and writes the message's length to `length` when it fails. A
/// result code other than zero fails the flush.
pub type DisunityGpuFlushCallback = unsafe extern "C" fn(
    world: u64,
    user: u64,
    message: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> DisunityResult;

/// Longest failure message kept from a callback, in bytes.
const MESSAGE_CAPACITY: u32 = 4096;

/// A registered callback and the word to hand it.
#[derive(Clone, Copy)]
pub(crate) struct GpuFlushHook {
    callback: DisunityGpuFlushCallback,
    user: u64,
}

/// Registers the callback a flush of `world` calls, replacing the one registered before; a null
/// callback removes it. `user` is handed back to the callback unchanged. The callback and what
/// `user` names must stay valid until it is replaced or removed or the world is destroyed.
///
/// # Safety
/// A non-null `callback` is a function with the signature of [`DisunityGpuFlushCallback`] that
/// may be called from any thread that flushes the world, and `user` is whatever it needs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_gpu_flush_register(
    world: u64,
    callback: Option<unsafe extern "C" fn(u64, u64, *mut u8, u32, *mut u32) -> DisunityResult>,
    user: u64,
) -> DisunityResult {
    crate::callbacks::mutate(world, |s| {
        s.gpu_flush = callback.map(|callback| GpuFlushHook { callback, user });
        Ok(())
    })
}

/// Registers the callback that every world created from now on starts with, replacing the default
/// registered before; a null callback removes the default. Worlds that exist already keep what
/// they have. A host that cannot register on a world before managed code runs in it, because the
/// managed side creates the world, registers the default first.
///
/// # Safety
/// A non-null `callback` is a function with the signature of [`DisunityGpuFlushCallback`] that
/// may be called from any thread that flushes a world, and `user` is whatever it needs. Both stay
/// valid until the default is replaced or removed and every world that started with it is gone.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_gpu_flush_register_default(
    callback: Option<unsafe extern "C" fn(u64, u64, *mut u8, u32, *mut u32) -> DisunityResult>,
    user: u64,
) -> DisunityResult {
    lock_store().default_gpu_flush = callback.map(|callback| GpuFlushHook { callback, user });
    DisunityResult::ok()
}

/// Runs the registered callback, if any, so that all pending offscreen work has executed when
/// this returns. Without a callback nothing happens and the compute buffers keep the contents
/// the CPU wrote.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_gpu_flush(world: u64) -> DisunityResult {
    let hook = {
        let mut store = lock_store();
        let Ok(session) = store.session_mut(world) else {
            return DisunityResult::invalid_handle();
        };
        match session.gpu_flush {
            None => return DisunityResult::ok(),
            Some(_) if session.flushing => None,
            Some(hook) => {
                session.flushing = true;
                Some(hook)
            }
        }
    };
    let Some(hook) = hook else {
        return translate_error(
            Some(world),
            &"the GPU flush was started from inside the GPU flush".into(),
        );
    };
    let mut message = vec![0_u8; MESSAGE_CAPACITY as usize];
    let mut length = 0_u32;
    let result = unsafe {
        (hook.callback)(
            world,
            hook.user,
            message.as_mut_ptr(),
            MESSAGE_CAPACITY,
            &mut length,
        )
    };
    if let Ok(session) = lock_store().session_mut(world) {
        session.flushing = false;
    }
    if result.code == 0 {
        return DisunityResult::ok();
    }
    let kept = message
        .get(..length.min(MESSAGE_CAPACITY) as usize)
        .unwrap_or_default();
    let text = String::from_utf8_lossy(kept);
    translate_error(Some(world), &format!("the GPU flush failed: {text}").into())
}
