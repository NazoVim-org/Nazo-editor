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
    commands_key: RegistryKey,
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

        // Build the Lua API table passed to setup(a).
        let api_table = self.lua.create_table().expect("failed to create Lua table");

        // We need a shared command registry: the `addCommand` callback stores
        // functions keyed by name into a Lua table referenced by self.commands_key.
        let cmd_registry = self.lua.create_table().expect("failed to create cmd table");
        // Replace the registry entry with our new table
        let _ = self
            .lua
            .set_named_registry_value("ijevim_cmd_registry", cmd_registry.clone());

        // The commands_key entry won't be used directly; use named registry instead.
        let _ = &self.commands_key;

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

        api_table
            .set(
                "on",
                self.lua
                    .create_function(|_lua, (_event_name, _handler): (String, Function)| {
                        // Event handlers not yet wired to Rust PluginEvent dispatch
                        Ok(())
                    })
                    .expect("failed to create on"),
            )
            .expect("failed to set on");

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

    fn handle_event(&mut self, _event: &PluginEvent) {}

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
        let plugin_table: mlua::Table = {
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
        }; // `value` dropped here, releasing the borrow on `lua`

        // Create an empty command registry table
        let cmd_table = lua
            .create_table()
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;
        let commands_key = lua
            .create_registry_value(cmd_table)
            .map_err(|e| super::LoaderError::Parse(format!("Registry error: {}", e)))?;

        let table_key = lua
            .create_registry_value(plugin_table)
            .map_err(|e| super::LoaderError::Parse(format!("Registry error: {}", e)))?;

        Ok(Box::new(LuaPlugin {
            name,
            lua,
            table_key,
            commands_key,
        }))
    }
}
