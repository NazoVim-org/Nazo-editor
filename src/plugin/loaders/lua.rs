use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use mlua::{Function, Lua, RegistryKey, Table};
use std::path::Path;
use std::rc::Rc;

pub struct LuaPlugin {
    name: String,
    lua: Lua,
    /// Key in the Lua registry referencing the plugin's returned table.
    table_key: RegistryKey,
    /// Key in the Lua registry referencing a table of command-name → function.
    #[allow(dead_code)]
    commands_key: RegistryKey,
    /// Key in the Lua registry referencing a table of event-name → [handler].
    #[allow(dead_code)]
    events_key: RegistryKey,
}

/// Map PluginEvent -> the Lua-side event name string used in `on()`.
fn plugin_event_name(event: &PluginEvent) -> Option<&'static str> {
    match event {
        PluginEvent::ModeChange { .. } => Some("mode_change"),
        PluginEvent::BufferChange => Some("buffer_change"),
        PluginEvent::Key { .. } => Some("key"),
        PluginEvent::BufferSave { .. } => Some("buffer_save"),
        PluginEvent::Ready => Some("editor:ready"),
    }
}

impl Plugin for LuaPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, api: &PluginApi) {
        let plugin_table: Table = match self.lua.registry_value(&self.table_key) {
            Ok(t) => t,
            Err(_) => return,
        };

        let setup_fn = match plugin_table.get::<_, Function>("setup") {
            Ok(f) => f,
            Err(_) => return,
        };

        let api_table = self.lua.create_table().expect("failed to create Lua table");

        // ── Command registry ─────────────────────────────────────
        let cmd_reg = self.lua.create_table().expect("failed to create cmd table");
        let _ = self
            .lua
            .set_named_registry_value("ijevim_cmd_registry", cmd_reg.clone());

        api_table
            .set(
                "addCommand",
                self.lua
                    .create_function(move |lua, (name, func): (String, Function)| {
                        let reg: Table = lua
                            .named_registry_value("ijevim_cmd_registry")
                            .unwrap_or_else(|_| lua.create_table().unwrap());
                        reg.set(name, func).ok();
                        lua.set_named_registry_value("ijevim_cmd_registry", reg)
                            .ok();
                        Ok(())
                    })
                    .expect("failed to create addCommand"),
            )
            .expect("failed to set addCommand");

        // ── Event registry ───────────────────────────────────────
        let ev_reg = self.lua.create_table().expect("failed to create ev table");
        let _ = self
            .lua
            .set_named_registry_value("ijevim_event_registry", ev_reg.clone());

        api_table
            .set(
                "on",
                self.lua
                    .create_function(move |lua, (event_name, handler): (String, Function)| {
                        let reg: Table = lua
                            .named_registry_value("ijevim_event_registry")
                            .unwrap_or_else(|_| lua.create_table().unwrap());
                        // Store as list: reg[event_name] = [handler, ...]
                        let list: Table = reg
                            .get::<_, Option<Table>>(event_name.as_str())
                            .unwrap_or(None)
                            .unwrap_or_else(|| lua.create_table().unwrap());
                        list.set(list.len().unwrap_or(0) + 1, handler).ok();
                        reg.set(event_name, list).ok();
                        lua.set_named_registry_value("ijevim_event_registry", reg)
                            .ok();
                        Ok(())
                    })
                    .expect("failed to create on"),
            )
            .expect("failed to set on");

        // ── Log ──────────────────────────────────────────────────
        let log_fn = api.log_fn.clone();
        api_table
            .set(
                "log",
                self.lua
                    .create_function(move |_lua, msg: String| {
                        (log_fn)(&msg);
                        Ok(())
                    })
                    .expect("failed to create log"),
            )
            .expect("failed to set log");

        let _ = setup_fn.call::<_, ()>(api_table);
    }

    fn handle_event(&mut self, event: &PluginEvent) {
        let name = match plugin_event_name(event) {
            Some(n) => n,
            None => return,
        };

        let reg: Table = match self.lua.named_registry_value("ijevim_event_registry") {
            Ok(t) => t,
            Err(_) => return,
        };

        let list: Table = match reg.get::<_, Option<Table>>(name) {
            Ok(Some(t)) => t,
            _ => return,
        };

        // Iterate over all handlers in the list
        for pair in list.pairs::<usize, Function>() {
            if let Ok((_idx, handler)) = pair {
                let _ = handler.call::<_, ()>(());
            }
        }
    }

    fn execute_command(&mut self, cmd: &str, _args: Vec<String>) -> bool {
        let reg: Table = match self.lua.named_registry_value("ijevim_cmd_registry") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let f: Function = match reg.get(cmd) {
            Ok(f) => f,
            Err(_) => return false,
        };
        f.call::<_, ()>(()).is_ok()
    }
}

pub struct LuaLoader;

impl super::Loader for LuaLoader {
    fn supported_extensions(&self) -> &[&str] {
        &["lua"]
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

        let lua = Lua::new();

        // Create the `ijevim` global table with log function
        let api_outer = api.clone();
        let ijevim_table = lua
            .create_table()
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        ijevim_table
            .set(
                "log",
                lua.create_function(move |_lua, msg: String| {
                    api_outer.log(&msg);
                    Ok(())
                })
                .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?,
            )
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        lua.globals()
            .set("ijevim", ijevim_table)
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        // Execute the plugin script; it should return a table with `name`, `setup`, etc.
        let plugin_table: Table = {
            let value: mlua::Value = lua
                .load(&code)
                .eval()
                .map_err(|e| super::LoaderError::Parse(format!("Lua eval error: {}", e)))?;
            match value {
                mlua::Value::Table(t) => t,
                _ => {
                    return Err(super::LoaderError::Parse(
                        "Lua plugin must return a table".to_string(),
                    ));
                }
            }
        };

        // Pre-register empty command and event tables
        let cmd_table = lua
            .create_table()
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;
        let commands_key = lua
            .create_registry_value(cmd_table)
            .map_err(|e| super::LoaderError::Parse(format!("Registry error: {}", e)))?;

        let ev_table = lua
            .create_table()
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;
        let events_key = lua
            .create_registry_value(ev_table)
            .map_err(|e| super::LoaderError::Parse(format!("Registry error: {}", e)))?;

        let table_key = lua
            .create_registry_value(plugin_table)
            .map_err(|e| super::LoaderError::Parse(format!("Registry error: {}", e)))?;

        Ok(Box::new(LuaPlugin {
            name,
            lua,
            table_key,
            commands_key,
            events_key,
        }))
    }
}
