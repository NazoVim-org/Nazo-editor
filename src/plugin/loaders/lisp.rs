use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use std::path::Path;
use std::rc::Rc;

pub struct LispPlugin {
    name: String,
}

impl Plugin for LispPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {}

    fn handle_event(&mut self, _event: &PluginEvent) {}

    fn execute_command(&mut self, _cmd: &str, _args: Vec<String>) -> bool {
        false
    }
}

pub struct LispLoader;

impl super::Loader for LispLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["lisp"]
    }

    fn load(
        &self,
        path: &Path,
        _api: Rc<PluginApi>,
    ) -> Result<Box<dyn Plugin>, super::LoaderError> {
        let _code = std::fs::read_to_string(path).map_err(|e| {
            super::LoaderError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        Ok(Box::new(LispPlugin { name }))
    }
}
