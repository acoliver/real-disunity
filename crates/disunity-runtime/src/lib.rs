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
mod bridge;
mod compatibility;
#[cfg(target_os = "linux")]
pub use compatibility::NativeImageLease;
pub use compatibility::{
    AdvanceReport, CapturedSnapshot, CompatibilityHost, HostIdleMutationError, SnapshotStamp,
};

use bridge::ManagedBridge;

/// Live-object snapshot row; mirrors the ffi ABI struct exactly.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityObjectState {
    pub object: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y_radians: f32,
    pub material_slot: u32,
}

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
    #[error("managed slice run failed with code {0}")]
    SliceRun(i32),
    #[error("disunity abi call failed with code {0}")]
    Abi(i32),
    #[error("no slice world is loaded")]
    NoWorld,
    #[error("managed compatibility dispatch failed: {0}")]
    Compatibility(String),
    #[error("native compatibility request failed: {0}")]
    Request(String),
    #[error("compatibility response exceeded its {0}-byte limit")]
    ResponseBudget(usize),
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

#[repr(C)]
#[derive(Clone, Copy)]
struct DisunityResult {
    code: i32,
    detail: u64,
}

/// Explicit application boundary. The bridge must expose the unmanaged `Run`,
/// `WorldHandle`, `Unload`, `TokenAlloc`, `TokenFree`, and `TokenLive` methods.
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

    #[cfg(target_os = "linux")]
    fn validate_managed(&self) -> Result<(), HostError> {
        for path in [&self.runtime_config, &self.assembly] {
            if !path.is_file() {
                return Err(HostError::ManagedArtifactsMissing(path.clone()));
            }
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
    // Native Rust pthread-key destructors can run after host drop or a boot error.
    // Linux keeps this selected image mapped through thread exit; worlds and managed
    // roots still require their ordinary cleanup. Other platforms retain their policy.
    #[cfg(target_os = "linux")]
    {
        use libloading::os::unix::{Library as UnixLibrary, RTLD_LAZY, RTLD_LOCAL};
        // Linux dlfcn.h extension, not exported by libloading 0.8.
        const RTLD_NODELETE: std::os::raw::c_int = 0x1000;
        // SAFETY: the caller explicitly selects a trusted native engine library.
        unsafe {
            UnixLibrary::open(
                Some(&config.ffi_library),
                RTLD_LAZY | RTLD_LOCAL | RTLD_NODELETE,
            )
        }
        .map(Library::from)
        .map_err(|error| HostError::FfiNotFound(error.to_string()))
    }
    #[cfg(not(target_os = "linux"))]
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
        let bridge = ManagedBridge::load(config)?;
        bridge.configure(config)?;

        Ok(Self {
            _library: library,
            export_objects,
            store_statistics,
            bridge,
            world: None,
            live_token: None,
            _claim: claim,
        })
    }

    /// Loads a world and builds the slice from `fixture`; the world stays
    /// live for [`DisunityHost::export_objects`] until
    /// [`DisunityHost::unload_slice`].
    pub fn run_slice(&mut self, fixture: &Path) -> Result<(), HostError> {
        if self.world.is_some() {
            return Err(HostError::WorldAlreadyLoaded);
        }
        let fixture = CString::new(fixture.to_string_lossy().as_bytes())
            .map_err(|_| HostError::SliceRun(-1))?;
        let token = unsafe { (self.bridge.token_alloc)() };
        self.live_token = Some(token);
        let code = unsafe { (self.bridge.run_slice)(fixture.as_ptr()) };
        if code != 0 {
            unsafe { (self.bridge.unload)() };
            unsafe { (self.bridge.token_free)(token) };
            self.live_token = None;
            return Err(HostError::SliceRun(code));
        }
        self.world = Some(unsafe { (self.bridge.world_handle)() });
        Ok(())
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
        unsafe { (self.bridge.unload)() };
        unsafe { (self.bridge.token_free)(token) };
        self.world = None;
        self.live_token = None;
        Ok(())
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

#[cfg(test)]
mod tests {
    use std::mem::{align_of, offset_of, size_of};

    use super::{DisunityObjectState, DisunityResult};

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

    #[cfg(target_os = "linux")]
    fn managed_config() -> super::ManagedHostConfig {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        super::ManagedHostConfig {
            ffi_library: manifest.join("missing-native-image-validation-fixture"),
            runtime_config: manifest.join("Cargo.toml"),
            assembly: manifest.join("Cargo.toml"),
            bridge_type: "Explicit.TestBridge, TestAssembly".into(),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn managed_only_validation_leaves_native_selection_to_the_owned_lease() {
        let config = managed_config();
        assert!(config.validate_managed().is_ok());
        assert!(matches!(
            config.validate(),
            Err(super::HostError::FfiNotFound(_))
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn legacy_validation_preserves_native_error_before_empty_bridge() {
        let mut config = managed_config();
        config.bridge_type = " 	".into();
        assert!(matches!(
            config.validate(),
            Err(super::HostError::FfiNotFound(_))
        ));
        assert!(matches!(
            config.validate_managed(),
            Err(super::HostError::Hostfxr(_))
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn both_validation_routes_reject_missing_managed_artifacts_first() {
        let mut config = managed_config();
        config.runtime_config = config.ffi_library.clone();
        assert!(matches!(
            config.validate(),
            Err(super::HostError::ManagedArtifactsMissing(path)) if path == config.runtime_config
        ));
        assert!(matches!(
            config.validate_managed(),
            Err(super::HostError::ManagedArtifactsMissing(path)) if path == config.runtime_config
        ));
    }
}
