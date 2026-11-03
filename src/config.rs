use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::{self, Language};

#[derive(Clone, Serialize, Deserialize)]
pub struct OpenWithRule {
    pub extension: String,
    pub command: String,
}

#[derive(Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub rules: Vec<OpenWithRule>,
}

fn default_language() -> String {
    i18n::system_default().code().to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: default_language(),
            rules: vec![OpenWithRule {
                extension: "*".to_string(),
                command: "{path}".to_string(),
            }],
        }
    }
}

impl Settings {
    pub fn language(&self) -> Language {
        Language::from_code(&self.language).unwrap_or_else(i18n::system_default)
    }
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("wff").join("config.json")
}

pub fn load() -> Settings {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Settings::default();
    };

    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let file = path();

    if let Some(directory) = file.parent() {
        std::fs::create_dir_all(directory)?;
    }

    let text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(file, text)
}
