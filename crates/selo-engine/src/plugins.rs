use crate::runtime;
use selo_plugin::PluginMeta;
use std::path::{Path, PathBuf};

pub struct Loaded {
    pub meta: PluginMeta,
    pub source: String,
    pub builtin: bool,
    pub path: Option<PathBuf>,
}

pub fn load_all(dir: impl AsRef<Path>, builtins: &[&str]) -> Vec<Loaded> {
    let mut plugins = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "js") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            match runtime::meta(&source) {
                Ok(meta) => plugins.push(Loaded {
                    meta,
                    source,
                    builtin: false,
                    path: Some(path),
                }),
                Err(err) => eprintln!("plugin {}: {err}", path.display()),
            }
        }
    }
    for source in builtins {
        match runtime::meta(source) {
            Ok(meta) if !plugins.iter().any(|other| other.meta.id == meta.id) => {
                plugins.push(Loaded {
                    meta,
                    source: (*source).to_string(),
                    builtin: true,
                    path: None,
                });
            }
            Ok(_) => {}
            Err(err) => eprintln!("builtin plugin: {err}"),
        }
    }
    plugins
}
