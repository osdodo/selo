use crate::{ConfigType, Error, PluginMeta, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const KEYRING_SERVICE: &str = "com.selo.app";

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    engine: Engine,
    #[serde(default)]
    ocr: Ocr,
    #[serde(default)]
    values: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    general: General,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Ocr {
    #[serde(default)]
    plugin: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct General {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    target_language: Option<String>,
    #[serde(default)]
    launch_at_login: bool,
    #[serde(default)]
    hotkey_translate: Option<String>,
    #[serde(default)]
    hotkey_capture: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Engine {
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    removed: Vec<String>,
    #[serde(default)]
    added: Vec<String>,
}

pub struct Settings {
    path: PathBuf,
    file: File,
}

impl Settings {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|err| Error::Parse(format!("{}: {err}", path.display())))?,
            Err(err) if err.kind() == ErrorKind::NotFound => File::default(),
            Err(source) => {
                return Err(Error::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        Ok(Self { path, file })
    }

    pub fn engine(&self) -> Option<&str> {
        self.file.engine.plugin.as_deref()
    }

    pub fn set_engine(&mut self, plugin: Option<String>) {
        self.file.engine.plugin = plugin;
    }

    pub fn ocr_engine(&self) -> Option<&str> {
        self.file.ocr.plugin.as_deref()
    }

    pub fn set_ocr_engine(&mut self, plugin: Option<String>) {
        self.file.ocr.plugin = plugin;
    }

    pub fn removed(&self) -> &[String] {
        &self.file.engine.removed
    }

    pub fn set_removed(&mut self, ids: Vec<String>) {
        self.file.engine.removed = ids;
    }

    pub fn added(&self) -> &[String] {
        &self.file.engine.added
    }

    pub fn set_added(&mut self, ids: Vec<String>) {
        self.file.engine.added = ids;
    }

    pub fn language(&self) -> Option<&str> {
        self.file.general.language.as_deref()
    }

    pub fn set_language(&mut self, value: Option<String>) {
        self.file.general.language = value;
    }

    pub fn target_language(&self) -> Option<&str> {
        self.file.general.target_language.as_deref()
    }

    pub fn set_target_language(&mut self, value: Option<String>) {
        self.file.general.target_language = value;
    }

    pub fn launch_at_login(&self) -> bool {
        self.file.general.launch_at_login
    }

    pub fn set_launch_at_login(&mut self, value: bool) {
        self.file.general.launch_at_login = value;
    }

    pub fn hotkey_translate(&self) -> Option<&str> {
        self.file.general.hotkey_translate.as_deref()
    }

    pub fn set_hotkey_translate(&mut self, value: Option<String>) {
        self.file.general.hotkey_translate = value;
    }

    pub fn hotkey_capture(&self) -> Option<&str> {
        self.file.general.hotkey_capture.as_deref()
    }

    pub fn set_hotkey_capture(&mut self, value: Option<String>) {
        self.file.general.hotkey_capture = value;
    }

    pub fn value(&self, plugin: &str, key: &str) -> Option<&str> {
        self.file.values.get(plugin)?.get(key).map(String::as_str)
    }

    pub fn set_value(&mut self, plugin: &str, key: &str, value: String) {
        self.file
            .values
            .entry(plugin.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    pub fn secret(&self, plugin: &str, key: &str) -> Option<String> {
        self.entry(plugin, key).ok()?.get_password().ok()
    }

    pub fn set_secret(&self, plugin: &str, key: &str, value: &str) -> Result<()> {
        self.entry(plugin, key)?
            .set_password(value)
            .map_err(|err| Error::Keyring(err.to_string()))
    }

    pub fn save(&self) -> Result<()> {
        let text =
            toml::to_string_pretty(&self.file).map_err(|err| Error::Parse(err.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Read {
                path: parent.display().to_string(),
                source,
            })?;
        }
        std::fs::write(&self.path, text).map_err(|source| Error::Read {
            path: self.path.display().to_string(),
            source,
        })
    }

    pub fn config_for_meta(&self, meta: &PluginMeta) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for field in &meta.config {
            let value = match field.kind {
                ConfigType::Secret => self.secret(&meta.id, &field.key),
                ConfigType::String => self.value(&meta.id, &field.key).map(str::to_string),
            };
            if let Some(value) = value {
                out.insert(field.key.clone(), value);
            }
        }
        out
    }

    fn entry(&self, plugin: &str, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, &format!("{plugin}.{key}"))
            .map_err(|err| Error::Keyring(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"
id = "my-gpt"
name = "My GPT"
version = "1.0.0"

[[config]]
key = "api_key"
label = "API Key"
type = "secret"
required = true

[[config]]
key = "model"
label = "Model"
type = "string"
"#;

    #[test]
    fn config_for_reads_file_values_and_leaves_secrets_absent_without_keyring() {
        let dir = std::env::temp_dir().join(format!("selo-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[engine]\nplugin = \"my-gpt\"\n\n[values.my-gpt]\nmodel = \"gpt-4o\"\n",
        )
        .unwrap();

        let settings = Settings::load(&path).unwrap();
        let meta: PluginMeta = toml::from_str(META).unwrap();
        assert_eq!(settings.engine(), Some("my-gpt"));

        let config = settings.config_for_meta(&meta);
        assert_eq!(config.get("model").map(String::as_str), Some("gpt-4o"));
        assert!(!config.contains_key("api_key"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_an_empty_settings_not_an_error() {
        let settings = Settings::load("/nonexistent/selo/config.toml").unwrap();
        assert_eq!(settings.engine(), None);
    }

    #[test]
    fn save_round_trips_non_secret_values() {
        let dir = std::env::temp_dir().join(format!("selo-save-{}", std::process::id()));
        let path = dir.join("config.toml");

        let mut settings = Settings::load(&path).unwrap();
        settings.set_engine(Some("my-gpt".into()));
        settings.set_ocr_engine(Some("ocr-plugin".into()));
        settings.set_language(Some("ja".into()));
        settings.set_value("my-gpt", "model", "gpt-4o-mini".into());
        settings.set_removed(vec!["google".into()]);
        settings.set_added(vec!["youdao".into()]);
        settings.save().unwrap();

        let reloaded = Settings::load(&path).unwrap();
        assert_eq!(reloaded.engine(), Some("my-gpt"));
        assert_eq!(reloaded.ocr_engine(), Some("ocr-plugin"));
        assert_eq!(reloaded.language(), Some("ja"));
        assert_eq!(reloaded.value("my-gpt", "model"), Some("gpt-4o-mini"));
        assert_eq!(reloaded.removed(), ["google"]);
        assert_eq!(reloaded.added(), ["youdao"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
