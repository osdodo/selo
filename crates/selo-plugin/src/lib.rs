mod settings;

use serde::Deserialize;

pub use settings::Settings;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("parse: {0}")]
    Parse(String),
    #[error("keyring: {0}")]
    Keyring(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub kind: PluginKind,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub no_proxy: bool,
    #[serde(default)]
    pub allow_hosts: Vec<String>,
    #[serde(default)]
    pub config: Vec<ConfigField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    #[default]
    Translate,
    Ocr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: ConfigType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub show_if: Option<ShowIf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShowIf {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigType {
    Secret,
    String,
}

fn default_timeout() -> u64 {
    30_000
}

pub fn host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hostnames() {
        assert_eq!(
            host("https://api.openai.com/v1/chat"),
            Some("api.openai.com")
        );
        assert_eq!(
            host("https://user:pw@Example.com:8443/x"),
            Some("Example.com")
        );
        assert_eq!(host("not a url"), None);
    }
}
