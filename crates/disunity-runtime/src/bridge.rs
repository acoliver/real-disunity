use std::ffi::{CString, OsStr, c_char};
use std::sync::OnceLock;

use netcorehost::hostfxr::{AssemblyDelegateLoader, Hostfxr};
use netcorehost::pdcstring::PdCString;

use crate::{HostError, ManagedHostConfig};

// CoreCLR cannot be unloaded. Keep its hosting library mapped across host lifetimes too.
static HOSTFXR: OnceLock<Hostfxr> = OnceLock::new();

fn pdcstring(value: &OsStr) -> Result<PdCString, HostError> {
    PdCString::from_os_str(value).map_err(|error| HostError::Hostfxr(error.to_string()))
}

pub(crate) fn managed_loader(
    config: &ManagedHostConfig,
) -> Result<AssemblyDelegateLoader, HostError> {
    let runtime_config = pdcstring(config.runtime_config.as_os_str())?;
    let assembly_path = pdcstring(config.assembly.as_os_str())?;
    let hostfxr = if let Some(hostfxr) = HOSTFXR.get() {
        hostfxr
    } else {
        let hostfxr = netcorehost::nethost::load_hostfxr()
            .map_err(|error| HostError::Hostfxr(error.to_string()))?;
        HOSTFXR.get_or_init(|| hostfxr)
    };
    let context = hostfxr
        .initialize_for_runtime_config(&runtime_config)
        .map_err(|error| HostError::Hostfxr(error.to_string()))?;
    context
        .get_delegate_loader_for_assembly(assembly_path)
        .map_err(|error| HostError::Hostfxr(error.to_string()))
}

pub(crate) struct ManagedBridge {
    pub run_slice: unsafe extern "system" fn(*const c_char) -> i32,
    pub world_handle: unsafe extern "system" fn() -> u64,
    pub unload: unsafe extern "system" fn(),
    pub token_alloc: unsafe extern "system" fn() -> isize,
    pub token_free: unsafe extern "system" fn(isize),
    pub token_live: unsafe extern "system" fn() -> i32,
    configure: unsafe extern "system" fn(*const c_char) -> i32,
}

impl ManagedBridge {
    pub fn load(config: &ManagedHostConfig) -> Result<Self, HostError> {
        let loader = managed_loader(config)?;
        let bridge_type = pdcstring(OsStr::new(&config.bridge_type))?;
        macro_rules! bind {
            ($name:literal, $signature:ty) => {
                *loader
                    .get_function_with_unmanaged_callers_only::<$signature>(
                        &bridge_type,
                        &pdcstring(OsStr::new($name))?,
                    )
                    .map_err(|error| HostError::Hostfxr(error.to_string()))?
            };
        }
        Ok(Self {
            run_slice: bind!("Run", unsafe extern "system" fn(*const c_char) -> i32),
            world_handle: bind!("WorldHandle", unsafe extern "system" fn() -> u64),
            unload: bind!("Unload", unsafe extern "system" fn()),
            token_alloc: bind!("TokenAlloc", unsafe extern "system" fn() -> isize),
            token_free: bind!("TokenFree", unsafe extern "system" fn(isize)),
            token_live: bind!("TokenLive", unsafe extern "system" fn() -> i32),
            configure: bind!(
                "ConfigureNativeLibrary",
                unsafe extern "system" fn(*const c_char) -> i32
            ),
        })
    }

    pub fn configure(&self, config: &ManagedHostConfig) -> Result<(), HostError> {
        let native_path = config
            .ffi_library
            .canonicalize()
            .map_err(|error| HostError::FfiNotFound(error.to_string()))?;
        let native_path = CString::new(native_path.to_string_lossy().as_bytes())
            .map_err(|error| HostError::FfiNotFound(error.to_string()))?;
        // SAFETY: the configured bridge follows the documented unmanaged contract.
        let code = unsafe { (self.configure)(native_path.as_ptr()) };
        if code != 0 {
            return Err(HostError::Abi(code));
        }
        Ok(())
    }
}
