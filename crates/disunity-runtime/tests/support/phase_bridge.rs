use netcorehost::{hostfxr::Hostfxr, pdcstring::PdCString};
use serde_json::Value;
use worldrunner_disunity_runtime::ManagedHostConfig;

type Entry = unsafe extern "C" fn(*const u8, usize) -> i32;

pub struct RawBridge {
    load: Entry,
    contact: Entry,
    unload: unsafe extern "C" fn() -> i32,
}

impl RawBridge {
    pub fn bind(config: &ManagedHostConfig) -> Self {
        static HOSTFXR: std::sync::OnceLock<Hostfxr> = std::sync::OnceLock::new();
        let hostfxr = HOSTFXR.get_or_init(|| netcorehost::nethost::load_hostfxr().unwrap());
        let runtime = PdCString::from_os_str(config.runtime_config.as_os_str()).unwrap();
        let assembly = PdCString::from_os_str(config.assembly.as_os_str()).unwrap();
        let context = hostfxr.initialize_for_runtime_config(&runtime).unwrap();
        let loader = context.get_delegate_loader_for_assembly(assembly).unwrap();
        let kind = PdCString::from_os_str(std::ffi::OsStr::new(&config.bridge_type)).unwrap();
        let name = PdCString::from_os_str(std::ffi::OsStr::new("LoadFixture")).unwrap();
        let pointer = *loader
            .get_function_with_unmanaged_callers_only::<unsafe extern "system" fn()>(&kind, &name)
            .unwrap();
        // SAFETY: the named trusted bridge entry exports exactly this Cdecl signature.
        let load = unsafe { std::mem::transmute::<unsafe extern "system" fn(), Entry>(pointer) };
        let name = PdCString::from_os_str(std::ffi::OsStr::new("Unload")).unwrap();
        let pointer = *loader
            .get_function_with_unmanaged_callers_only::<unsafe extern "system" fn()>(&kind, &name)
            .unwrap();
        // SAFETY: this named trusted bridge entry exports Cdecl with no arguments.
        let unload = unsafe {
            std::mem::transmute::<unsafe extern "system" fn(), unsafe extern "C" fn() -> i32>(
                pointer,
            )
        };
        let name = PdCString::from_os_str(std::ffi::OsStr::new("InvokeContact")).unwrap();
        let pointer = *loader
            .get_function_with_unmanaged_callers_only::<unsafe extern "system" fn()>(&kind, &name)
            .unwrap();
        // SAFETY: the named trusted bridge entry exports exactly this Cdecl signature.
        let contact = unsafe { std::mem::transmute::<unsafe extern "system" fn(), Entry>(pointer) };
        Self {
            load,
            contact,
            unload,
        }
    }

    pub fn contact_on_thread(&self, request: &Value) -> i32 {
        let bytes = serde_json::to_vec(request).unwrap();
        let contact = self.contact;
        std::thread::spawn(move || {
            // SAFETY: the selected images stay mapped until this joined Cdecl call returns.
            unsafe { contact(bytes.as_ptr(), bytes.len()) }
        })
        .join()
        .unwrap()
    }

    pub fn unload(&self) -> i32 {
        // SAFETY: the mapped bridge retains this no-argument Cdecl entry.
        unsafe { (self.unload)() }
    }

    pub fn load(&self, manifest: &Value) -> i32 {
        let bytes = serde_json::to_vec(manifest).unwrap();
        // SAFETY: the managed entry borrows these UTF-8 bytes synchronously.
        unsafe { (self.load)(bytes.as_ptr(), bytes.len()) }
    }
}
