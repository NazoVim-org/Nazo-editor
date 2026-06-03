use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use quickjs_rusty::Context;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

thread_local! {
    /// Thread-local API + state reference for JS callbacks.
    /// Closures capture nothing (access via thread-local),
    /// so they are `RefUnwindSafe` and `'static`.
    static JS_CTX: RefCell<Option<(Rc<PluginApi>, Rc<RefCell<JsPluginState>>)>> =
        const { RefCell::new(None) };
}

struct JsPluginState {
    commands: Vec<String>,
    events: Vec<String>,
}

impl JsPluginState {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            events: Vec::new(),
        }
    }
}

pub struct JavaScriptPlugin {
    name: String,
    state: Rc<RefCell<JsPluginState>>,
}

impl Plugin for JavaScriptPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {}

    fn handle_event(&mut self, event: &PluginEvent) {
        let event_name = event_name_str(event);
        let state = self.state.borrow();
        if state.events.iter().any(|e| e == event_name) {
            let _ = event_name;
        }
    }

    fn execute_command(&mut self, cmd: &str, _args: Vec<String>) -> bool {
        let state = self.state.borrow();
        state.commands.iter().any(|c| c == cmd)
    }
}

fn event_name_str(event: &PluginEvent) -> &'static str {
    match event {
        PluginEvent::Ready => "Ready",
        PluginEvent::Key { .. } => "Key",
        PluginEvent::BufferChange => "BufferChange",
        PluginEvent::BufferSave { .. } => "BufferSave",
        PluginEvent::ModeChange { .. } => "ModeChange",
    }
}

pub struct JavaScriptLoader;

impl super::Loader for JavaScriptLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["js"]
    }

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let code = std::fs::read_to_string(path).map_err(|e| {
            super::LoaderError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let ctx = Context::builder().build().map_err(|e| {
            super::LoaderError::Parse(format!("Failed to create JS context: {}", e))
        })?;

        // Create shared state
        let state = Rc::new(RefCell::new(JsPluginState::new()));

        // Store API + state in thread-local for callback access.
        // Closures capture nothing, so they are `RefUnwindSafe` and `'static`.
        JS_CTX.with(|slot| {
            *slot.borrow_mut() = Some((api.clone(), state.clone()));
        });

        // Register global callbacks.
        ctx.add_callback("__ijevim_addCommand", |cmd: String| -> String {
            JS_CTX.with(|slot| {
                if let Some((ref _api, ref state)) = *slot.borrow() {
                    state.borrow_mut().commands.push(cmd.clone());
                }
            });
            format!("Command '{}' registered", cmd)
        })
        .map_err(|e| super::LoaderError::Parse(format!("Failed to register addCommand: {}", e)))?;

        ctx.add_callback("__ijevim_on", |event: String| -> String {
            JS_CTX.with(|slot| {
                if let Some((ref _api, ref state)) = *slot.borrow() {
                    state.borrow_mut().events.push(event.clone());
                }
            });
            format!("Handler for '{}' registered", event)
        })
        .map_err(|e| super::LoaderError::Parse(format!("Failed to register on: {}", e)))?;

        ctx.add_callback("__ijevim_log", |msg: String| -> String {
            JS_CTX.with(|slot| {
                if let Some((ref api, ref _state)) = *slot.borrow() {
                    api.log(&msg);
                }
            });
            String::new()
        })
        .map_err(|e| super::LoaderError::Parse(format!("Failed to register log: {}", e)))?;

        // JS preamble: create the ijevim namespace wrapping the flat callbacks
        let preamble = r#"
var ijevim = (function() {
    return {
        addCommand: function(cmd) { return __ijevim_addCommand(cmd); },
        on: function(event) { return __ijevim_on(event); },
        log: function(msg) { __ijevim_log(msg); }
    };
})();
"#;
        ctx.eval(preamble, false)
            .map_err(|e| super::LoaderError::Parse(format!("JS preamble eval error: {}", e)))?;

        // Evaluate the plugin code
        ctx.eval(&code, false)
            .map_err(|e| super::LoaderError::Parse(format!("JS eval error: {}", e)))?;

        // Clear thread-local
        JS_CTX.with(|slot| {
            *slot.borrow_mut() = None;
        });

        Ok(Box::new(JavaScriptPlugin { name, state }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::loaders::Loader;
    use crate::plugin::PluginApi;

    #[test]
    fn test_js_loader_basic() {
        let dir = std::env::temp_dir().join("ijevim-test-js");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.js");
        std::fs::write(&path, "ijevim.log(\"hello\");").unwrap();

        let loader = JavaScriptLoader;
        let api = Rc::new(PluginApi::new());
        let plugin = loader.load(&path, api).unwrap();
        assert_eq!(plugin.name(), "test");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_js_loader_register_command() {
        let dir = std::env::temp_dir().join("ijevim-test-js-cmd");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cmdtest.js");
        std::fs::write(&path, "ijevim.addCommand(\"hello-js\");").unwrap();

        let loader = JavaScriptLoader;
        let api = Rc::new(PluginApi::new());
        let mut plugin = loader.load(&path, api.clone()).unwrap();

        assert!(plugin.execute_command("hello-js", vec![]));
        assert!(!plugin.execute_command("unknown", vec![]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_js_loader_register_event() {
        let dir = std::env::temp_dir().join("ijevim-test-js-evt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("evttest.js");
        std::fs::write(&path, "ijevim.on(\"Ready\");").unwrap();

        let loader = JavaScriptLoader;
        let api = Rc::new(PluginApi::new());
        let _plugin = loader.load(&path, api).unwrap();
    }
}
