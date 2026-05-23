// pub mod javascript;
pub mod lisp;
pub mod lua;
pub mod nix;
pub mod rust;

use crate::plugin::{Plugin, PluginApi};
use std::path::Path;
use std::rc::Rc;

pub const API_VERSION: u32 = 1;

#[derive(Debug)]
pub enum LoaderError {
    Io(String),
    Parse(String),
    UnsupportedLanguage(String),
    ApiVersionMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::Io(s) => write!(f, "IO error: {}", s),
            LoaderError::Parse(s) => write!(f, "Parse error: {}", s),
            LoaderError::UnsupportedLanguage(s) => write!(f, "Unsupported language: {}", s),
            LoaderError::ApiVersionMismatch { expected, actual } => {
                write!(
                    f,
                    "API version mismatch: expected v{}, got v{}",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for LoaderError {}

pub trait Loader: Send + Sync {
    fn supported_extensions(&self) -> &[&str];

    fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, LoaderError>;
}

pub struct LoaderRegistry {
    loaders: Vec<Box<dyn Loader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self {
        Self {
            loaders: Vec::new(),
        }
    }

    pub fn register(&mut self, loader: Box<dyn Loader>) {
        self.loaders.push(loader);
    }

    pub fn load(&self, path: &Path, api: Rc<PluginApi>) -> Result<Box<dyn Plugin>, LoaderError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| LoaderError::UnsupportedLanguage("no extension".to_string()))?;

        for loader in &self.loaders {
            if loader.supported_extensions().contains(&ext) {
                return loader.load(path, api);
            }
        }

        Err(LoaderError::UnsupportedLanguage(ext.to_string()))
    }

}

impl Default for LoaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn create_default_registry() -> LoaderRegistry {
    let mut registry = LoaderRegistry::new();
    registry.register(Box::new(lua::LuaLoader));
    registry.register(Box::new(lisp::LispLoader));
    // registry.register(Box::new(javascript::JavaScriptLoader));
    registry.register(Box::new(nix::NixLoader));
    registry.register(Box::new(rust::RustLoader));
    registry
}
