//! The synchronous GPU flush. A host registers a closure that executes the offscreen work scripts
//! recorded, dispatches included; the managed engine calls it when a script reads a result back,
//! so the results exist when the read returns.
//!
//! The closure runs inside the call that reads the result back, with no lock of the engine held,
//! so [`GpuFlush`] can call the same engine image that the managed code is running in.
use std::panic::{AssertUnwindSafe, catch_unwind};

use libloading::Library;

use crate::{DisunityHost, HostError, native};

type R = native::DisunityResult;

/// What a flush does: runs the pending work and writes results. An error is logged by the managed
/// engine and the managed call that asked for the flush goes on without the results.
pub type GpuFlushHandler = Box<dyn FnMut(&mut GpuFlush<'_>) -> Result<(), String>>;

/// The function the engine image calls to flush; see `disunity_gpu_flush_register`.
type Trampoline = unsafe extern "C" fn(u64, u64, *mut u8, u32, *mut u32) -> R;

/// The engine functions a flush calls, resolved once from the image the host retains.
pub(crate) struct Calls {
    pub(crate) snapshot: crate::graphics_snapshot::SnapshotCall,
    pub(crate) shader:
        unsafe extern "C" fn(u64, u64, *mut native::DisunityShaderInfo, *mut u8) -> R,
    pub(crate) material: unsafe extern "C" fn(u64, u64, *mut native::DisunityMaterialInfo) -> R,
    pub(crate) render_texture:
        unsafe extern "C" fn(u64, u64, *mut native::DisunityRenderTexture) -> R,
    pub(crate) property:
        unsafe extern "C" fn(u64, u32, u64, *const u8, u32, *mut native::DisunityPropertyRead) -> R,
    pub(crate) keyword: unsafe extern "C" fn(u64, u32, u64, *const u8, u32, *mut u32) -> R,
    pub(crate) compute_shader: unsafe extern "C" fn(
        u64,
        u64,
        *mut native::DisunityComputeShaderInfo,
        *mut native::DisunityComputeKernel,
        *mut u8,
    ) -> R,
    pub(crate) buffer_info:
        unsafe extern "C" fn(u64, u64, *mut native::DisunityComputeBufferInfo) -> R,
    pub(crate) buffer_read: unsafe extern "C" fn(u64, u64, u64, *mut u8, u32) -> R,
    pub(crate) buffer_write: unsafe extern "C" fn(u64, u64, u64, *const u8, u32) -> R,
    pub(crate) kernel_size: unsafe extern "C" fn(u64, u64, u32, u32, u32, u32) -> R,
    pub(crate) pixels_write: unsafe extern "C" fn(u64, u64, *const f32, u32) -> R,
    pub(crate) mesh_channels: crate::mesh_channels::Read,
    pub(crate) texture: crate::textures::Read,
    pub(crate) sampler: crate::textures::SamplerRead,
    error_take: unsafe extern "C" fn(u64, *mut native::DisunityBuffer) -> R,
    buffer_copy: unsafe extern "C" fn(u64, *mut u8, u32, *mut u32) -> R,
    buffer_free: unsafe extern "C" fn(u64) -> R,
}

/// Resolves a symbol of the image the host retains.
pub(crate) fn symbol<T: Copy>(library: &Library, name: &'static [u8]) -> Result<T, HostError> {
    // SAFETY: the caller names the exported function the type `T` describes.
    unsafe {
        library.get::<T>(name).map(|symbol| *symbol).map_err(|_| {
            HostError::MissingSymbol(String::from_utf8_lossy(name).trim_end_matches('\0').into())
        })
    }
}

impl Calls {
    fn load(library: &Library) -> Result<Self, HostError> {
        Ok(Self {
            snapshot: symbol(library, b"disunity_graphics_snapshot\0")?,
            shader: symbol(library, b"disunity_shader_read\0")?,
            material: symbol(library, b"disunity_material_read\0")?,
            render_texture: symbol(library, b"disunity_render_texture_read\0")?,
            property: symbol(library, b"disunity_shader_property_read\0")?,
            keyword: symbol(library, b"disunity_shader_keyword_read\0")?,
            compute_shader: symbol(library, b"disunity_compute_shader_read\0")?,
            buffer_info: symbol(library, b"disunity_compute_buffer_info\0")?,
            buffer_read: symbol(library, b"disunity_compute_buffer_read\0")?,
            buffer_write: symbol(library, b"disunity_compute_buffer_write\0")?,
            kernel_size: symbol(library, b"disunity_compute_kernel_size_write\0")?,
            pixels_write: symbol(library, b"disunity_render_texture_pixels_write\0")?,
            mesh_channels: symbol(library, b"disunity_mesh_channels_read\0")?,
            texture: symbol(library, b"disunity_texture_read\0")?,
            sampler: symbol(library, b"disunity_texture_sampler_read\0")?,
            error_take: symbol(library, b"disunity_error_take\0")?,
            buffer_copy: symbol(library, b"disunity_buffer_copy\0")?,
            buffer_free: symbol(library, b"disunity_buffer_free\0")?,
        })
    }
}

/// What a flush handler can do to the world being flushed. Every method goes through the engine
/// image's ABI, so it sees what the managed engine has stored and what the handler wrote before.
pub struct GpuFlush<'a> {
    pub(crate) world: u64,
    pub(crate) calls: &'a Calls,
}

impl GpuFlush<'_> {
    /// The ABI handle of the world being flushed.
    pub fn world(&self) -> u64 {
        self.world
    }

    /// Turns a failed result into the error with the world's own message.
    pub(crate) fn check(&self, result: R) -> Result<(), HostError> {
        if result.code == 0 {
            return Ok(());
        }
        let mut owned = native::DisunityBuffer::default();
        let taken = unsafe { (self.calls.error_take)(self.world, &mut owned) };
        if taken.code != 0 {
            return Err(HostError::Abi(taken.code));
        }
        let mut bytes = vec![0; owned.length as usize];
        let mut written = 0;
        let copied = unsafe {
            (self.calls.buffer_copy)(owned.handle, bytes.as_mut_ptr(), owned.length, &mut written)
        };
        let freed = unsafe { (self.calls.buffer_free)(owned.handle) };
        if copied.code != 0 || freed.code != 0 || written != owned.length {
            return Err(HostError::Abi(2));
        }
        Err(HostError::Native {
            code: result.code,
            reason: crate::decode_reason(result.detail),
            message: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }
}

/// The registered handler and the functions it uses. The engine image holds a pointer to it, so
/// it lives on the heap until the host replaces or drops it.
pub(crate) struct FlushContext {
    calls: Calls,
    handler: GpuFlushHandler,
}

unsafe extern "C" fn trampoline(
    world: u64,
    user: u64,
    message: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> R {
    // SAFETY: `user` is the context pointer registered for this world, alive until the host
    // unregisters it, and the engine image calls this on the thread of the flush.
    let context = unsafe { &mut *(user as *mut FlushContext) };
    let mut flush = GpuFlush {
        world,
        calls: &context.calls,
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| (context.handler)(&mut flush)));
    let text = match outcome {
        Ok(Ok(())) => return native::DisunityResult { code: 0, detail: 0 },
        Ok(Err(text)) => text,
        Err(_) => "the flush handler panicked".to_owned(),
    };
    let kept = text.len().min(capacity as usize);
    // SAFETY: the engine image passes a buffer of `capacity` bytes and a length to write.
    unsafe {
        std::ptr::copy_nonoverlapping(text.as_ptr(), message, kept);
        *length = u32::try_from(kept).unwrap_or(0);
    }
    native::DisunityResult { code: 3, detail: 0 }
}

impl DisunityHost {
    /// Sets the handler a flush calls, replacing the one set before. The managed engine flushes
    /// when a script reads back a compute buffer or a kernel's thread group size; the handler
    /// takes the pending work ([`GpuFlush::snapshot`]) and executes it. A handler that
    /// returns an error does not fail the managed call: the engine logs the text once per
    /// distinct text and the call goes on with the contents the CPU holds. Without a handler a flush does nothing and a compute buffer keeps the
    /// contents the CPU wrote.
    ///
    /// The handler applies to the world of the running slice and to the worlds of later slices,
    /// including a slice whose managed code reads back results while it loads.
    pub fn set_gpu_flush(&mut self, handler: GpuFlushHandler) -> Result<(), HostError> {
        let calls = Calls::load(&self._library)?;
        let context = Box::into_raw(Box::new(FlushContext { calls, handler }));
        if let Err(error) = self.register_everywhere(context) {
            // SAFETY: the engine image never kept this pointer, or was told to forget it.
            drop(unsafe { Box::from_raw(context) });
            return Err(error);
        }
        let old = std::mem::replace(&mut self.gpu_flush, context);
        if !old.is_null() {
            // SAFETY: the pointer came from `Box::into_raw` and the engine image now holds the
            // new one; no flush runs while the host is borrowed.
            drop(unsafe { Box::from_raw(old) });
        }
        Ok(())
    }

    /// Registers the context for worlds made later and for the running world.
    fn register_everywhere(&self, context: *mut FlushContext) -> Result<(), HostError> {
        self.register_default(context)?;
        if let Some(world) = self.world {
            self.register_flush(world, context)?;
        }
        Ok(())
    }

    fn register_default(&self, context: *mut FlushContext) -> Result<(), HostError> {
        let register: unsafe extern "C" fn(Option<Trampoline>, u64) -> R =
            symbol(&self._library, b"disunity_gpu_flush_register_default\0")?;
        let callback = (!context.is_null()).then_some(trampoline as Trampoline);
        let result = unsafe { register(callback, context as u64) };
        if result.code == 0 {
            Ok(())
        } else {
            Err(HostError::Abi(result.code))
        }
    }

    /// Registers the context (or nothing, for null) with the engine image for `world`.
    fn register_flush(&self, world: u64, context: *mut FlushContext) -> Result<(), HostError> {
        let register: unsafe extern "C" fn(u64, Option<Trampoline>, u64) -> R =
            symbol(&self._library, b"disunity_gpu_flush_register\0")?;
        let callback = (!context.is_null()).then_some(trampoline as Trampoline);
        let result = unsafe { register(world, callback, context as u64) };
        if result.code == 0 {
            Ok(())
        } else {
            Err(self.native_error(result))
        }
    }

    /// Removes the handler from the world before it is unloaded.
    pub(crate) fn detach_gpu_flush(&self, world: u64) {
        if !self.gpu_flush.is_null() {
            let _ = self.register_flush(world, std::ptr::null_mut());
        }
    }

    /// Frees the handler when the host goes away, after the engine image forgets it.
    pub(crate) fn free_gpu_flush(&mut self) {
        let context = std::mem::replace(&mut self.gpu_flush, std::ptr::null_mut());
        if !context.is_null() {
            let _ = self.register_default(std::ptr::null_mut());
            // SAFETY: detached from every world first, and the pointer came from `Box::into_raw`.
            drop(unsafe { Box::from_raw(context) });
        }
    }
}
