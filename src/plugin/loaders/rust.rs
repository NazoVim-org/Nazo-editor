use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use libloading::{Library, Symbol};
use std::path::Path;
use std::rc::Rc;

const PLUGIN_VERSION: u32 = super::API_VERSION;

#[allow(improper_ctypes_definitions)]
type CreatePluginFn = unsafe extern "C" fn() -> *mut dyn Plugin;
type GetApiVersionFn = unsafe extern "C" fn() -> u32;

pub struct RustPlugin {
    inner: Box<dyn Plugin>,
}

impl Plugin for RustPlugin {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn setup(&mut self, api: &PluginApi) {
        self.inner.setup(api);
    }

    fn handle_event(&mut self, event: &PluginEvent) {
        self.inner.handle_event(event);
    }

    fn execute_command(&mut self, cmd: &str, args: Vec<String>) -> bool {
        self.inner.execute_command(cmd, args)
    }
}

pub struct RustLoader;

impl super::Loader for RustLoader {
    fn name(&self) -> &str {
        "rust"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["so", "dylib", "dll"]
    }

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let library = unsafe {
            Library::new(path)
                .map_err(|e| super::LoaderError::Io(format!("Failed to load library: {}", e)))?
        };

        unsafe {
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

            let create: Symbol<CreatePluginFn> =
                library.get(b"ijevim_plugin_create").map_err(|e| {
                    super::LoaderError::Io(format!("Failed to get create symbol: {}", e))
                })?;

            let plugin_ptr = create();

            if plugin_ptr.is_null() {
                return Err(super::LoaderError::Parse(
                    "Plugin creation returned null".to_string(),
                ));
            }

            let mut plugin = Box::from_raw(plugin_ptr);
            plugin.setup(&api);

            Ok(Box::new(RustPlugin { inner: plugin }))
        }
    }
}
