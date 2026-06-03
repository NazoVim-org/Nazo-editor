mod api;
pub mod loaders;

pub use api::{Plugin, PluginApi};

use crate::types::{IjevimError, PluginEvent};
use std::path::Path;
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

    /// Config directory: `$XDG_CONFIG_HOME/ijevim/` or `~/.config/ijevim/`.
    fn config_dir() -> std::path::PathBuf {
        std::env::var("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").expect("HOME not set");
                std::path::PathBuf::from(home).join(".config")
            })
            .join("ijevim")
    }

    /// Plugin directory: `$NESTVIM_PLUGIN_DIR` or `$CONFIG_DIR/plugins/`.
    fn plugins_dir() -> std::path::PathBuf {
        if let Ok(dir) = std::env::var("NESTVIM_PLUGIN_DIR") {
            std::path::PathBuf::from(dir)
        } else {
            Self::config_dir().join("plugins")
        }
    }

    /// Load all plugins from init files and plugins directory.
    ///
    /// 1. Load `init.{lua,lisp,js,nix}` from config dir (first match wins per ext).
    /// 2. Load all files from plugins/ directory (extension-based dispatch).
    /// 3. Call `setup()` on every loaded plugin.
    /// 4. Emit `PluginEvent::Ready`.
    pub fn load_all(&mut self) -> Result<(), IjevimError> {
        // Step 1: load init files from config dir
        let config_dir = Self::config_dir();
        if config_dir.exists() {
            for ext in &["lua", "lisp", "js", "nix"] {
                let init_path = config_dir.join(format!("init.{}", ext));
                if init_path.exists() {
                    self.load_single(&init_path);
                }
            }
        }

        // Step 2: load all files from plugins/ directory
        let plugins_dir = Self::plugins_dir();
        if plugins_dir.exists() {
            let entries = std::fs::read_dir(&plugins_dir).map_err(IjevimError::Io)?;
            for entry in entries {
                let entry = entry.map_err(IjevimError::Io)?;
                self.load_single(&entry.path());
            }
        }

        // Step 3 & 4: setup all plugins and emit Ready
        self.setup_all();
        self.emit(PluginEvent::Ready);

        Ok(())
    }

    fn load_single(&mut self, path: &Path) {
        match self.registry.load(path, self.api.clone()) {
            Ok(plugin) => {
                let name = plugin.name().to_string();
                self.plugins.push(plugin);
                eprintln!("[plugin] Loaded: {}", name);
            }
            Err(e) => {
                eprintln!("[plugin] Failed to load {}: {}", path.display(), e);
            }
        }
    }

    /// Call `setup(&api)` on every plugin that hasn't been set up yet.
    /// (Plugins that were set up by their loader keep a flag, but currently
    /// every loader defers to this method.)
    fn setup_all(&mut self) {
        for plugin in &mut self.plugins {
            plugin.setup(&self.api);
        }
    }

    pub fn emit(&mut self, event: PluginEvent) {
        for plugin in &mut self.plugins {
            plugin.handle_event(&event);
        }

        let handlers = self.api.event_handlers();
        let event_name = match &event {
            PluginEvent::ModeChange { .. } => "ModeChange",
            PluginEvent::BufferChange => "BufferChange",
            PluginEvent::Key { .. } => "Key",
            PluginEvent::BufferSave { .. } => "BufferSave",
            PluginEvent::Ready => "Ready",
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
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
