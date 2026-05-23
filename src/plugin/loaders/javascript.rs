use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use quickjs_rusty::Context;
use std::path::Path;
use std::rc::Rc;

pub struct JavaScriptPlugin {
    name: String,
    #[allow(dead_code)]
    context: Context,
}

impl Plugin for JavaScriptPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {}

    fn handle_event(&mut self, _event: &PluginEvent) {}

    fn execute_command(&mut self, _cmd: &str, _args: Vec<String>) -> bool {
        false
    }
}

pub struct JavaScriptLoader;

impl super::Loader for JavaScriptLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["js", "mjs"]
    }

    fn load(
        &self,
        path: &Path,
        _api: Rc<PluginApi>,
    ) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let code = std::fs::read_to_string(path).map_err(|e| {
            super::LoaderError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let context = Context::builder()
            .build()
            .map_err(|e| super::LoaderError::Parse(format!("Context creation failed: {}", e)))?;

        let js_code = format!(
            r#"
            var ijevim = {{
                log: function(msg) {{ console.log(msg); }}
            }};
            {}"#,
            code
        );

        let _ = context.eval(&js_code, false);

        Ok(Box::new(JavaScriptPlugin { name, context }))
    }
}
