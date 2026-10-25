//! Live managed callbacks over the same dynamically loaded native world image.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::mem::ManuallyDrop;

use libloading::Library;
use netcorehost::pdcstring::PdCString;
use serde_json::Value;

use crate::{DisunityResult, HostClaim, HostError, ManagedHostConfig, bridge, load_ffi_image};

mod advance;
mod cleanup;
mod dispatch;
mod exact;
mod failures;
mod handshake;
mod idle_cleanup;
mod idle_mutation;
pub use idle_mutation::HostIdleMutationError;
mod phase;
mod readiness;
mod snapshot;
mod terminal;
pub use advance::AdvanceReport;
#[cfg(test)]
mod terminal_tests;
pub use failures::CallbackFailurePage;
pub use snapshot::{CapturedSnapshot, SnapshotStamp};
mod transport;

use transport::NativeTransport;

type JsonEntry = unsafe extern "C" fn(*const u8, usize) -> i32;
type CopyEntry = unsafe extern "C" fn(*mut u8, usize, *mut usize) -> i32;
type ObserveEntry = unsafe extern "C" fn(*const u8, usize, *mut u8, usize, *mut usize) -> i32;

const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;

struct CompatibilityBridge {
    dispatch_depth: std::cell::Cell<usize>,
    describe: CopyEntry,
    configure: JsonEntry,
    load: JsonEntry,
    phase: JsonEntry,
    contact: JsonEntry,
    observe: ObserveEntry,
    error: CopyEntry,
    unload: unsafe extern "C" fn() -> i32,
}

impl CompatibilityBridge {
    fn bind(config: &ManagedHostConfig) -> Result<Self, HostError> {
        let loader = bridge::managed_loader(config)?;
        let bridge_type = PdCString::from_os_str(OsStr::new(&config.bridge_type))
            .map_err(|error| HostError::Hostfxr(error.to_string()))?;
        macro_rules! entry {
            ($name:literal, $signature:ty) => {{
                let name = PdCString::from_os_str(OsStr::new($name))
                    .map_err(|error| HostError::Hostfxr(error.to_string()))?;
                let entry = *loader
                    .get_function_with_unmanaged_callers_only::<unsafe extern "system" fn()>(
                        &bridge_type,
                        &name,
                    )
                    .map_err(|error| HostError::Hostfxr(error.to_string()))?;
                // netcorehost types all returned pointers as System ABI. The selected
                // bridge explicitly exports Cdecl; reinterpret before any invocation.
                // SAFETY: each trusted entry name has the paired managed Cdecl signature.
                unsafe { std::mem::transmute::<unsafe extern "system" fn(), $signature>(entry) }
            }};
        }
        Ok(Self {
            dispatch_depth: std::cell::Cell::new(0),
            describe: entry!("DescribeBridge", CopyEntry),
            configure: entry!("ConfigureNativeLibrary", JsonEntry),
            load: entry!("LoadFixture", JsonEntry),
            phase: entry!("DispatchPhase", JsonEntry),
            contact: entry!("InvokeContact", JsonEntry),
            observe: entry!("Observe", ObserveEntry),
            error: entry!("GetLastError", CopyEntry),
            unload: entry!("Unload", unsafe extern "C" fn() -> i32),
        })
    }

    fn description(&self) -> Result<Value, HostError> {
        let mut required = 0;
        // SAFETY: DescribeBridge is pure and permits a null-buffer probe.
        let probe = unsafe { (self.describe)(std::ptr::null_mut(), 0, &mut required) };
        if probe != -2 {
            self.check(probe)?;
        }
        if required == 0 || required > RESPONSE_LIMIT {
            return Err(HostError::Compatibility(
                "compatibility handshake: invalid bridge descriptor size".into(),
            ));
        }
        let mut bytes = vec![0; required];
        // SAFETY: the pure entry copies into this bounded caller-owned allocation.
        self.check(unsafe { (self.describe)(bytes.as_mut_ptr(), bytes.len(), &mut required) })?;
        if required != bytes.len() {
            return Err(HostError::Compatibility(
                "compatibility handshake: descriptor length changed".into(),
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| HostError::Compatibility(format!("compatibility handshake: {error}")))
    }

    fn check(&self, code: i32) -> Result<(), HostError> {
        if code == 0 {
            return Ok(());
        }
        let mut required = 0;
        // SAFETY: the bridge permits a null-buffer length probe and retains no pointers.
        unsafe { (self.error)(std::ptr::null_mut(), 0, &mut required) };
        if required > RESPONSE_LIMIT {
            return Err(HostError::ResponseBudget(RESPONSE_LIMIT));
        }
        let mut bytes = vec![0; required];
        // SAFETY: the allocated buffer has the probed capacity.
        unsafe { (self.error)(bytes.as_mut_ptr(), bytes.len(), &mut required) };
        let message = String::from_utf8_lossy(&bytes);
        Err(HostError::Compatibility(format!("code {code}: {message}")))
    }

    fn invoke(&self, entry: JsonEntry, value: &Value) -> Result<(), HostError> {
        let _dispatch = self.enter_dispatch()?;
        let bytes = transport::codec::encode_bridge(value)?;
        // SAFETY: the bridge reads this UTF-8 buffer only during the synchronous call.
        self.check(unsafe { entry(bytes.as_ptr(), bytes.len()) })
    }
}

/// One managed bridge bound to one native image. The caller owns frame sequencing.
/// Native requests are complete before managed callbacks begin, so callbacks may
/// enter the native store without retaining a lock across either language boundary.
pub struct CompatibilityHost {
    bridge: CompatibilityBridge,
    native: ManuallyDrop<NativeTransport>,
    negotiation: ManuallyDrop<handshake::Negotiation>,
    worlds: ManuallyDrop<BTreeSet<u64>>,
    loaded: bool,
    loaded_world: Option<u64>,
    strict: ManuallyDrop<BTreeMap<u64, phase::StrictSession>>,
    _library: ManuallyDrop<Library>,
    _claim: ManuallyDrop<HostClaim>,
    _thread_affinity: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl CompatibilityHost {
    pub fn boot(config: &ManagedHostConfig) -> Result<Self, HostError> {
        config.validate()?;
        let claim = HostClaim::acquire()?;
        let library = load_ffi_image(config)?;
        let native = NativeTransport::bind(&library)?;
        let description = native.request(0, &serde_json::json!({"version":1,"op":"describe"}))?;
        handshake::validate_native(&description)?;
        let bridge = CompatibilityBridge::bind(config)?;
        let negotiation = handshake::Negotiation::new(description, bridge.description()?)?;
        let path = config
            .ffi_library
            .canonicalize()
            .map_err(|error| HostError::FfiNotFound(error.to_string()))?;
        let bytes = path.to_string_lossy();
        // SAFETY: the configured bridge consumes the explicit UTF-8 native image path.
        bridge.check(unsafe { (bridge.configure)(bytes.as_ptr(), bytes.len()) })?;
        Ok(Self {
            bridge,
            native: ManuallyDrop::new(native),
            negotiation: ManuallyDrop::new(negotiation),
            worlds: ManuallyDrop::new(BTreeSet::new()),
            loaded: false,
            loaded_world: None,
            strict: ManuallyDrop::new(BTreeMap::new()),
            _library: ManuallyDrop::new(library),
            _claim: ManuallyDrop::new(claim),
            _thread_affinity: std::marker::PhantomData,
        })
    }

    pub fn create_world(&mut self) -> Result<u64, HostError> {
        let world = self.native.create_world()?;
        if let Err(error) = self.validate_world(world) {
            let _ = self.native.destroy_world(world);
            return Err(error);
        }
        self.worlds.insert(world);
        Ok(world)
    }

    pub fn destroy_world(&mut self, world: u64) -> Result<(), HostError> {
        if !self.worlds.contains(&world) {
            return Err(HostError::NoWorld);
        }
        if self.loaded_world == Some(world) {
            self.unload()?;
        }
        if self.strict.contains_key(&world) {
            self.close_phase_session(world)?;
        }
        self.retry_cleanup(world)?;
        self.native.destroy_world(world)?;
        self.strict.remove(&world);
        self.worlds.remove(&world);
        Ok(())
    }

    /// Executes one native operation; copying its response never repeats mutation.
    pub fn request(&self, world: u64, request: &Value) -> Result<Value, HostError> {
        if self.strict.get(&world).is_some_and(|s| s.outcome_unknown)
            && !matches!(
                request["op"].as_str(),
                Some("session.status" | "object.get" | "time.get" | "input.get" | "snapshot")
            )
        {
            self.require_resolved_idle(world)?;
        }
        self.native.request(world, request)
    }

    pub fn load_fixture(&mut self, fixture: &Value) -> Result<(), HostError> {
        if self.loaded {
            return Err(HostError::WorldAlreadyLoaded);
        }
        let world = self.negotiation.manifest(fixture)?;
        self.validate_world(world)?;
        self.require_resolved_idle(world)?;
        idle_mutation::validate_selection(self, world, fixture)?;
        self.worlds.insert(world);
        if self.strict.contains_key(&world) {
            return self.load_strict(world, fixture);
        }
        self.loaded = true;
        self.loaded_world = Some(world);
        if let Err(error) = self.bridge.invoke(self.bridge.load, fixture) {
            let _ = self.unload();
            return Err(error);
        }
        self.ensure_no_callback_failures()
    }

    pub fn dispatch_phase(&mut self, phase: &Value) -> Result<(), HostError> {
        self.require_loaded()?;
        self.bridge.invoke(self.bridge.phase, phase)?;
        self.ensure_no_callback_failures()
    }

    pub fn invoke_contact(&mut self, contact: &Value) -> Result<(), HostError> {
        self.require_loaded()?;
        self.bridge.invoke(self.bridge.contact, contact)?;
        self.ensure_no_callback_failures()
    }

    pub fn observe(&self, query: &Value) -> Result<Value, HostError> {
        if !matches!(
            query["op"].as_str(),
            Some("managed.statistics" | "managed.failures")
        ) {
            self.require_loaded()?;
        }
        let request = transport::codec::encode_bridge(query)?;
        let mut required = 0;
        // SAFETY: Observe is read-only and supports null-buffer capacity probing.
        let probe = unsafe {
            (self.bridge.observe)(
                request.as_ptr(),
                request.len(),
                std::ptr::null_mut(),
                0,
                &mut required,
            )
        };
        if probe != -2 {
            self.bridge.check(probe)?;
        }
        if required == 0 {
            return Err(HostError::Request("managed observation was empty".into()));
        }
        if required > RESPONSE_LIMIT {
            return Err(HostError::ResponseBudget(RESPONSE_LIMIT));
        }
        let mut bytes = vec![0; required];
        // SAFETY: the read-only result is copied into a bounded caller-owned allocation.
        let code = unsafe {
            (self.bridge.observe)(
                request.as_ptr(),
                request.len(),
                bytes.as_mut_ptr(),
                bytes.len(),
                &mut required,
            )
        };
        self.bridge.check(code)?;
        if required != bytes.len() {
            return Err(HostError::Request(
                "managed observation length changed during copy".into(),
            ));
        }
        serde_json::from_slice(&bytes).map_err(|error| HostError::Request(error.to_string()))
    }

    /// Reads retained failures without acknowledging or draining them.
    pub fn callback_failures(&self, after: u64) -> Result<CallbackFailurePage, HostError> {
        let response = self.observe(&serde_json::json!({"op":"managed.failures","after":after}))?;
        CallbackFailurePage::parse(&response)
    }

    /// A consumed Cdecl batch can succeed while application callbacks failed.
    pub fn ensure_no_callback_failures(&self) -> Result<(), HostError> {
        let page = self.callback_failures(0)?;
        if page.total == 0 {
            return Ok(());
        }
        Err(HostError::Compatibility(format!(
            "managed callback failures: total={} retained={} dropped={}; {:?}",
            page.total, page.retained, page.dropped, page.records
        )))
    }

    pub fn unload(&mut self) -> Result<(), HostError> {
        if !self.loaded {
            return Ok(());
        }
        if let Some(world) = self.loaded_world.filter(|w| self.strict.contains_key(w)) {
            return self.unload_strict(world);
        }
        // SAFETY: the bridge is still mapped and owns only its managed dispatch roots.
        let code = unsafe { (self.bridge.unload)() };
        self.loaded = false;
        let world = self.loaded_world.take();
        let closed = if let Some(world) = world.filter(|w| self.strict.contains_key(w)) {
            self.close_phase_session(world)
        } else {
            Ok(())
        };
        self.bridge.check(code).and(closed)
    }

    pub fn store_statistics(&self) -> Result<(u64, u64), HostError> {
        self.native.statistics()
    }

    fn validate_world(&self, world: u64) -> Result<(), HostError> {
        let description = self
            .native
            .request(world, &serde_json::json!({"version":1,"op":"describe"}))
            .map_err(|error| {
                HostError::Compatibility(format!("compatibility handshake: {error}"))
            })?;
        self.negotiation.validate_world(&description)
    }

    fn require_loaded(&self) -> Result<(), HostError> {
        if let Some(world) = self.loaded_world {
            self.require_resolved_idle(world)?;
        }
        self.loaded.then_some(()).ok_or(HostError::NoWorld)
    }
}

impl Drop for CompatibilityHost {
    fn drop(&mut self) {
        let mut errors = Vec::new();
        if !self.strict.values().any(|s| s.outcome_unknown)
            && let Err(error) = self.unload()
        {
            errors.push(error);
        }
        errors.extend(self.drain_without_callbacks());
        if self.worlds.is_empty() && !self.loaded && errors.is_empty() {
            // SAFETY: native worlds and measured managed roots are drained.
            unsafe { self.release_fields() };
        } else {
            self.retain_failed_drop(&errors);
        }
    }
}

fn check_native(result: DisunityResult) -> Result<(), HostError> {
    if result.code == 0 {
        Ok(())
    } else {
        Err(HostError::Abi(result.code))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod idle_cleanup_tests;

#[cfg(all(test, target_os = "linux"))]
mod idle_cleanup_fault_tests;

#[cfg(all(test, target_os = "linux"))]
mod idle_cleanup_uncertainty_tests;

#[cfg(test)]
mod capability_tests;
