//! The CoreCLR host for Disunity: boots a configured .NET runtime, loads the
//! managed assemblies through hostfxr, and drives one shared
//! `worldrunner_disunity_ffi` image for both sides of the boundary.
//!
//! The single-image rule is structural: this crate never links the
//! `worldrunner-disunity-ffi` rlib. All engine-store access goes through the
//! same cdylib the C# `DllImport`s resolve to, so C# and Rust observe one
//! object table instead of two divergent copies.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use libloading::Library;
mod managed_entrypoints;

use managed_entrypoints::ManagedBridge;

#[path = "native.g.rs"]
pub mod native;
pub use native::DisunityErrorReason;
pub use native::DisunityObjectState;
use native::{DisunityAdvance, DisunityResult};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("disunity ffi library not found at the configured path: {0}")]
    FfiNotFound(String),
    #[error("native symbol {0} missing from the disunity ffi library")]
    MissingSymbol(String),
    #[error("managed assembly or runtime configuration not found: {0}")]
    ManagedArtifactsMissing(PathBuf),
    #[error("a managed host is already active in this process")]
    AlreadyBooted,
    #[error("a world is already loaded")]
    WorldAlreadyLoaded,
    #[error("hostfxr failed to load a .NET runtime: {0}")]
    Hostfxr(String),
    #[error("managed slice run failed with code {code}: {message}")]
    SliceRun { code: i32, message: String },
    #[error("disunity abi call failed with code {0}")]
    Abi(i32),
    #[error("native operation failed ({code}/{reason:?}): {message}")]
    Native {
        code: i32,
        reason: DisunityErrorReason,
        message: String,
    },
    #[error("ABI revision mismatch: expected {expected}, got {actual}")]
    AbiRevision { expected: u32, actual: u32 },
    #[error("managed callback failed: {0}")]
    Managed(String),
    #[error("no slice world is loaded")]
    NoWorld,
}

static CLAIMED: AtomicBool = AtomicBool::new(false);

struct HostClaim;

impl HostClaim {
    fn acquire() -> Result<Self, HostError> {
        CLAIMED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| HostError::AlreadyBooted)?;
        Ok(Self)
    }
}

impl Drop for HostClaim {
    fn drop(&mut self) {
        CLAIMED.store(false, Ordering::SeqCst);
    }
}

/// Explicit application boundary. The bridge must expose the unmanaged `Run`,
/// `WorldHandle`, `Unload`, `Dispatch`, `Error`, `ReadPosition`,
/// `ConfigureNativeLibrary`, `TokenAlloc`, `TokenFree`, and `TokenLive` methods.
/// `Run` receives a UTF-8 fixture path; no application assembly is selected implicitly.
#[derive(Clone, Debug)]
pub struct ManagedHostConfig {
    pub ffi_library: PathBuf,
    pub runtime_config: PathBuf,
    pub assembly: PathBuf,
    pub bridge_type: String,
}

impl ManagedHostConfig {
    fn validate(&self) -> Result<(), HostError> {
        for path in [&self.runtime_config, &self.assembly] {
            if !path.is_file() {
                return Err(HostError::ManagedArtifactsMissing(path.clone()));
            }
        }
        if !self.ffi_library.is_file() {
            return Err(HostError::FfiNotFound(
                self.ffi_library.display().to_string(),
            ));
        }
        if self.bridge_type.trim().is_empty() {
            return Err(HostError::Hostfxr(
                "bridge type must be explicit and nonempty".into(),
            ));
        }
        Ok(())
    }
}

fn load_ffi_image(config: &ManagedHostConfig) -> Result<Library, HostError> {
    // SAFETY: the caller explicitly selects a trusted native engine library.
    unsafe {
        Library::new(&config.ffi_library).map_err(|error| HostError::FfiNotFound(error.to_string()))
    }
}

macro_rules! symbol {
    ($lib:expr, $name:literal) => {{
        const NAME: &[u8] = $name;
        unsafe {
            *$lib
                .get(NAME)
                .map_err(|_| HostError::MissingSymbol(String::from_utf8_lossy(NAME).into_owned()))?
        }
    }};
}

/// One booted host. One CLR exists per process, so hosts create exactly one
/// and run any number of slice lifecycles through it.
pub struct DisunityHost {
    _library: Library,
    advance: unsafe extern "C" fn(u64, u64, u64, *mut DisunityAdvance) -> DisunityResult,
    resume:
        unsafe extern "C" fn(u64, u64, u32, *const u8, u32, *mut DisunityAdvance) -> DisunityResult,
    error_take: unsafe extern "C" fn(u64, *mut native::DisunityBuffer) -> DisunityResult,
    buffer_copy: unsafe extern "C" fn(u64, *mut u8, u32, *mut u32) -> DisunityResult,
    buffer_free: unsafe extern "C" fn(u64) -> DisunityResult,
    callback_check: unsafe extern "C" fn(u64, u64) -> DisunityResult,
    set_position: unsafe extern "C" fn(u64, u64, f32, f32, f32) -> DisunityResult,
    export_objects:
        unsafe extern "C" fn(u64, *mut DisunityObjectState, u32, *mut u32) -> DisunityResult,
    store_statistics: unsafe extern "C" fn(*mut u64, *mut u64) -> DisunityResult,
    bridge: ManagedBridge,
    /// The world of the in-flight slice run; cleared by [`DisunityHost::unload_slice`].
    world: Option<u64>,
    /// GCHandle pinned for the in-flight run; freed on unload. Kept
    /// engine-side so a leak is observable here, not only in managed memory.
    live_token: Option<isize>,
    _claim: HostClaim,
    _thread_affinity: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl DisunityHost {
    /// Loads the ffi image, boots .NET from the slice assembly's
    /// runtimeconfig, and binds the slice's `[UnmanagedCallersOnly]` entries.
    ///
    /// The configuration must identify trusted native and managed code.
    /// Only one host may own the process-wide managed bridge at a time.
    pub fn boot(config: &ManagedHostConfig) -> Result<Self, HostError> {
        config.validate()?;
        let claim = HostClaim::acquire()?;
        let library = load_ffi_image(config)?;
        // SAFETY: symbols resolve against the explicitly configured ABI library.
        let export_objects = symbol!(library, b"disunity_world_export_objects\0");
        let store_statistics = symbol!(library, b"disunity_store_statistics\0");
        let version: unsafe extern "C" fn(*mut u32) -> DisunityResult =
            symbol!(library, b"disunity_abi_version\0");
        let mut revision = 0;
        let result = unsafe { version(&mut revision) };
        if result.code != 0 {
            return Err(HostError::Abi(result.code));
        }
        check_revision(revision)?;
        let bridge = ManagedBridge::load(config)?;
        bridge.configure(config)?;

        Ok(Self {
            advance: symbol!(library, b"disunity_world_advance\0"),
            resume: symbol!(library, b"disunity_world_resume\0"),
            error_take: symbol!(library, b"disunity_error_take\0"),
            buffer_copy: symbol!(library, b"disunity_buffer_copy\0"),
            buffer_free: symbol!(library, b"disunity_buffer_free\0"),
            callback_check: symbol!(library, b"disunity_callback_check\0"),
            set_position: symbol!(library, b"disunity_transform_set_position\0"),
            _library: library,
            export_objects,
            store_statistics,
            bridge,
            world: None,
            live_token: None,
            _claim: claim,
            _thread_affinity: std::marker::PhantomData,
        })
    }

    /// Loads a world and builds the slice from `fixture`; the world stays
    /// live for [`DisunityHost::export_objects`] until
    /// [`DisunityHost::unload_slice`].
    pub fn run_slice(&mut self, fixture: &Path) -> Result<(), HostError> {
        if self.world.is_some() {
            return Err(HostError::WorldAlreadyLoaded);
        }
        let fixture = CString::new(fixture.to_string_lossy().as_bytes()).map_err(|_| {
            HostError::SliceRun {
                code: -1,
                message: "the slice path contains a NUL byte".into(),
            }
        })?;
        let token = unsafe { (self.bridge.token_alloc)() };
        self.live_token = Some(token);
        let code = unsafe { (self.bridge.run_slice)(fixture.as_ptr()) };
        if code != 0 {
            // Read the failure before unloading, which may replace it.
            let message = self.bridge.failure();
            unsafe { (self.bridge.unload)() };
            unsafe { (self.bridge.token_free)(token) };
            self.live_token = None;
            return Err(HostError::SliceRun { code, message });
        }
        self.world = Some(unsafe { (self.bridge.world_handle)() });
        Ok(())
    }

    /// Advances the native scheduler, releasing the ABI call before managed dispatch.
    pub fn advance(&mut self, numerator: u64, denominator: u64) -> Result<(), HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let mut work = DisunityAdvance::default();
        let mut result = unsafe { (self.advance)(world, numerator, denominator, &mut work) };
        let mut failure = None;
        while result.code == 0 && work.kind == 1 {
            let code = unsafe { (self.bridge.dispatch)(work.script, work.message) };
            let message = if code == 0 {
                String::new()
            } else {
                self.bridge.failure()
            };
            if failure.is_none() && code != 0 {
                failure = Some(message.clone());
            }
            result = unsafe { (self.callback_check)(world, work.ticket) };
            if result.code != 0 {
                break;
            }
            result = unsafe {
                (self.resume)(
                    world,
                    work.ticket,
                    u32::from(code != 0),
                    message.as_ptr(),
                    u32::try_from(message.len()).map_err(|_| HostError::Abi(2))?,
                    &mut work,
                )
            };
        }
        if let Some(message) = failure {
            return Err(HostError::Managed(message));
        }
        if result.code != 0 {
            return Err(self.native_error(result));
        }
        Ok(())
    }

    fn native_error(&self, result: DisunityResult) -> HostError {
        let mut owned = native::DisunityBuffer::default();
        let taken = unsafe { (self.error_take)(self.world.unwrap_or(0), &mut owned) };
        if taken.code != 0 {
            return HostError::Abi(taken.code);
        }
        let mut bytes = vec![0; owned.length as usize];
        let mut written = 0;
        let copied = unsafe {
            (self.buffer_copy)(owned.handle, bytes.as_mut_ptr(), owned.length, &mut written)
        };
        let freed = unsafe { (self.buffer_free)(owned.handle) };
        if copied.code != 0 || freed.code != 0 || written != owned.length {
            return HostError::Abi(2);
        }
        HostError::Native {
            code: result.code,
            reason: decode_reason(result.detail),
            message: String::from_utf8(bytes).expect("native diagnostics are UTF-8"),
        }
    }

    /// Mutates the very same native object used by managed wrappers.
    pub fn set_position(&mut self, object: u64, value: [f32; 3]) -> Result<(), HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let result = unsafe { (self.set_position)(world, object, value[0], value[1], value[2]) };
        if result.code == 0 {
            Ok(())
        } else {
            Err(self.native_error(result))
        }
    }

    pub fn managed_position(&self, object: u64) -> Result<[f32; 3], HostError> {
        self.world.ok_or(HostError::NoWorld)?;
        let mut value = [0.0; 3];
        let code = unsafe { (self.bridge.read_position)(object, value.as_mut_ptr()) };
        if code == 0 {
            Ok(value)
        } else {
            Err(HostError::Managed(self.bridge.failure()))
        }
    }

    /// Bulk-extracts live object states; the renderer's per-frame read.
    pub fn export_objects(&self) -> Result<Vec<DisunityObjectState>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let mut written = 0_u32;
        // SAFETY: capacity-probe call with a null buffer; written is a valid out-ptr.
        let code = unsafe { (self.export_objects)(world, std::ptr::null_mut(), 0, &mut written) };
        if code.code != 0 {
            return Err(HostError::Abi(code.code));
        }
        let mut states = vec![
            DisunityObjectState {
                object: 0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                rotation_y_radians: 0.0,
                material_slot: 0,
            };
            written as usize
        ];
        let mut filled = 0_u32;
        // SAFETY: states has `written` rows; filled is a valid out-ptr.
        let code =
            unsafe { (self.export_objects)(world, states.as_mut_ptr(), written, &mut filled) };
        if code.code != 0 || filled != written {
            return Err(HostError::Abi(code.code));
        }
        Ok(states)
    }

    /// Ends the slice: unloads the managed world and frees the run's token.
    pub fn unload_slice(&mut self) -> Result<(), HostError> {
        let token = self.live_token.ok_or(HostError::NoWorld)?;
        let code = unsafe { (self.bridge.unload)() };
        unsafe { (self.bridge.token_free)(token) };
        self.world = None;
        self.live_token = None;
        if code == 0 {
            Ok(())
        } else {
            Err(HostError::Managed(self.bridge.failure()))
        }
    }

    /// Live and allocated world-slot counts, the soak drain check.
    pub fn store_statistics(&self) -> (u64, u64) {
        let (mut live, mut allocated) = (0_u64, 0_u64);
        // SAFETY: both out-pointers are valid stack locals.
        unsafe { (self.store_statistics)(&mut live, &mut allocated) };
        (live, allocated)
    }

    /// Managed-side live token count, a genuine Rust-to-C# read.
    pub fn token_live(&self) -> i32 {
        unsafe { (self.bridge.token_live)() }
    }

    /// Engine-side token state; the soak asserts it drains to unheld.
    pub fn engine_token_live(&self) -> bool {
        self.live_token.is_some()
    }
}

impl Drop for DisunityHost {
    fn drop(&mut self) {
        if self.live_token.is_some() {
            let _ = self.unload_slice();
        }
    }
}

fn check_revision(actual: u32) -> Result<(), HostError> {
    let expected = native::DISUNITY_ABI_VERSION;
    if actual == expected {
        Ok(())
    } else {
        Err(HostError::AbiRevision { expected, actual })
    }
}

fn decode_reason(reason: u64) -> DisunityErrorReason {
    use DisunityErrorReason as R;
    match reason {
        2 => R::InvalidTime,
        3 => R::WrongCallback,
        4 => R::CallbackFailed,
        5 => R::Faulted,
        6 => R::AbortedScope,
        7 => R::Capacity,
        8 => R::SingularParent,
        9 => R::Physics,
        10 => R::StaleHandle,
        11 => R::AdvanceInProgress,
        12 => R::InvalidComponent,
        13 => R::Ui,
        14 => R::NotReady,
        _ => R::InvalidArgument,
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::{DisunityObjectState, DisunityResult};

    #[test]
    fn accepts_only_the_generated_revision() {
        let current = super::native::DISUNITY_ABI_VERSION;
        assert!(super::check_revision(current).is_ok());
        for other in [0, current - 1, current + 1] {
            assert!(super::check_revision(other).is_err(), "{other}");
        }
    }

    #[test]
    fn dynamically_loaded_native_types_match_the_abi_layout() {
        assert_eq!(size_of::<DisunityResult>(), 16);
        assert_eq!(align_of::<DisunityResult>(), 8);
        assert_eq!(offset_of!(DisunityResult, code), 0);
        assert_eq!(offset_of!(DisunityResult, detail), 8);
        assert_eq!(size_of::<DisunityObjectState>(), 32);
        assert_eq!(align_of::<DisunityObjectState>(), 8);
        assert_eq!(offset_of!(DisunityObjectState, object), 0);
        assert_eq!(offset_of!(DisunityObjectState, x), 8);
        assert_eq!(offset_of!(DisunityObjectState, y), 12);
        assert_eq!(offset_of!(DisunityObjectState, z), 16);
        assert_eq!(offset_of!(DisunityObjectState, rotation_y_radians), 20);
        assert_eq!(offset_of!(DisunityObjectState, material_slot), 24);
    }
}

mod sprites;

mod input;
mod presentation;
