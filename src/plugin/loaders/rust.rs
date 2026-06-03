use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use libloading::{Library, Symbol};
use std::ffi::CStr;
use std::path::Path;
use std::rc::Rc;

const PLUGIN_VERSION: u32 = super::API_VERSION;

/// FFI-safe vtable that a Rust plugin `.so` must export.
///
/// All function pointers use stable C ABI. The plugin is responsible for
/// managing its own state behind the opaque `*mut c_void` handle.
#[repr(C)]
struct PluginVtable {
    /// Return a human-readable name (null-terminated UTF-8). The string must
    /// remain valid for the lifetime of the plugin handle.
    get_name: unsafe extern "C" fn() -> *const std::ffi::c_char,
    /// Run the plugin's setup routine.
    setup: unsafe extern "C" fn(),
    /// Handle an editor event.
    handle_event: unsafe extern "C" fn(event_type: u32, data: usize),
    /// Execute a named command. Returns 1 if handled, 0 otherwise.
    execute_command: unsafe extern "C" fn(cmd: *const u8, cmd_len: usize) -> u8,
}

type GetApiVersionFn = unsafe extern "C" fn() -> u32;
type GetVtableFn = unsafe extern "C" fn() -> PluginVtable;

pub struct RustPlugin {
    /// Keep the library alive so function pointers remain valid.
    _library: Library,
    vtable: PluginVtable,
    name: String,
}

impl Plugin for RustPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {
        unsafe {
            (self.vtable.setup)();
        }
    }

    fn handle_event(&mut self, _event: &PluginEvent) {}

    fn execute_command(&mut self, cmd: &str, _args: Vec<String>) -> bool {
        let cmd_bytes = cmd.as_bytes();
        unsafe { (self.vtable.execute_command)(cmd_bytes.as_ptr(), cmd_bytes.len()) != 0 }
    }
}

pub struct RustLoader;

impl super::Loader for RustLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["so", "dylib", "dll"]
    }

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let library = unsafe {
            Library::new(path)
                .map_err(|e| super::LoaderError::Io(format!("Failed to load library: {}", e)))?
        };

        unsafe {
            // Check API version first
            let version: Symbol<GetApiVersionFn> =
                library.get(b"ijevim_plugin_api_version").map_err(|e| {
                    super::LoaderError::Io(format!("Failed to get version symbol: {}", e))
                })?;

            let actual_version = version();
            if actual_version != PLUGIN_VERSION {
                return Err(super::LoaderError::ApiVersionMismatch {
                    expected: PLUGIN_VERSION,
                    actual: actual_version,
                });
            }

            // Load the vtable
            let get_vtable: Symbol<GetVtableFn> =
                library.get(b"ijevim_plugin_vtable").map_err(|e| {
                    super::LoaderError::Io(format!("Failed to get vtable symbol: {}", e))
                })?;

            let vtable = get_vtable();
            let name = {
                let ptr = (vtable.get_name)();
                if ptr.is_null() {
                    return Err(super::LoaderError::Parse("Plugin name is null".to_string()));
                }
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            };

            api.log(&format!("Loaded Rust plugin: {}", name));

            Ok(Box::new(RustPlugin {
                _library: library,
                vtable,
                name,
            }))
        }
    }
}
