use crate::plugin::{Plugin, PluginApi};
use crate::types::PluginEvent;
use mlua::Lua;
use std::path::Path;
use std::rc::Rc;

pub struct LuaPlugin {
    name: String,
    #[allow(dead_code)]
    lua: Lua,
}

impl Plugin for LuaPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn setup(&mut self, _api: &PluginApi) {}

    fn handle_event(&mut self, _event: &PluginEvent) {}

    fn execute_command(&mut self, _cmd: &str, _args: Vec<String>) -> bool {
        false
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

        let api_outer = api.clone();
        #[allow(unused_must_use)]
        lua.globals().set(
            "ijevim",
            lua.create_table()
                .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?,
        );

        lua.globals()
            .get::<_, mlua::Table>("ijevim")
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?
            .set(
                "log",
                lua.create_function(move |_lua, msg: String| {
                    api_outer.log(&msg);
                    Ok(())
                })
                .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?,
            )
            .map_err(|e| super::LoaderError::Parse(format!("Lua error: {}", e)))?;

        let _ = lua
            .load(&code)
            .eval::<mlua::Value>()
            .map_err(|e| super::LoaderError::Parse(format!("Lua eval error: {}", e)))?;

        Ok(Box::new(LuaPlugin { name, lua }))
    }
}
