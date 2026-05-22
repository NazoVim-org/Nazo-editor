mod api;
pub mod loaders;

pub use api::{Plugin, PluginApi};

use crate::types::{NestvimError, PluginEvent};
use std::rc::Rc;

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    api: Rc<PluginApi>,
    registry: loaders::LoaderRegistry,
}

impl PluginManager {
    pub fn new() -> Self {
        let api = Rc::new(PluginApi::new());

        let registry = loaders::create_default_registry();

        Self {
            plugins: Vec::new(),
            api,
            registry,
        }
    }

    pub fn load_all(&mut self) -> Result<(), NestvimError> {
        let plugins_dir = if let Ok(dir) = std::env::var("NESTVIM_PLUGIN_DIR") {
            std::path::PathBuf::from(dir)
        } else {
            let config_dir = std::env::var("XDG_CONFIG_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").expect("HOME environment variable not set");
                    std::path::PathBuf::from(home).join(".config")
                });
            config_dir.join("ijevim").join("plugins")
        };

        if !plugins_dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(&plugins_dir).map_err(NestvimError::Io)?;

        for entry in entries {
            let entry = entry.map_err(NestvimError::Io)?;
            let path = entry.path();

            match self.registry.load(&path, self.api.clone()) {
                Ok(plugin) => {
                    let name = plugin.name().to_string();
                    self.add_plugin(plugin);
                    eprintln!("[plugin] Loaded: {}", name);
                }
                Err(e) => {
                    eprintln!("[plugin] Failed to load {}: {}", path.display(), e);
                }
            }
        }

        Ok(())
    }

    pub fn emit(&mut self, event: PluginEvent) {
        for plugin in &mut self.plugins {
            plugin.handle_event(&event);
        }

        let handlers = self.api.event_handlers();
        let event_name = match &event {
            PluginEvent::ModeChange { .. } => "mode_change",
            PluginEvent::BufferChange => "buffer_change",
            PluginEvent::Key { .. } => "key",
            PluginEvent::BufferSave { .. } => "buffer_save",
        };
        if let Some(event_handlers) = handlers.borrow().get(event_name) {
            for handler in event_handlers {
                handler(&event);
            }
        }
    }

    pub fn execute_command(&mut self, cmd: &str) -> bool {
        for plugin in &mut self.plugins {
            if plugin.execute_command(cmd, vec![]) {
                return true;
            }
        }

        let commands = self.api.commands();
        if let Some(f) = commands.borrow().get(cmd) {
            f(vec![]);
            true
        } else {
            false
        }
    }

    pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    #[allow(dead_code)]
    pub fn api(&self) -> &Rc<PluginApi> {
        &self.api
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
