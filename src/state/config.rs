use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KtermConfig {
    #[serde(default)]
    pub font: FontConfig,
    pub colors: ColorConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: String::from("FiraCode Nerd Font"),
            size: 16.0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ColorConfig {
    pub background: [u8; 3],
    pub foreground: [u8; 3],
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            background: [18, 18, 18],
            foreground: [220, 220, 220],
        }
    }
}

impl Default for KtermConfig {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            colors: ColorConfig::default(),
        }
    }
}

impl KtermConfig {
    pub fn load() -> Self {
        let mut config_path = match dirs::config_dir() {
            Some(path) => path,
            None => return Self::default(),
        };

        config_path.push("kterm");

        if !config_path.exists() {
            let _ = fs::create_dir_all(&config_path);
        }

        config_path.push("kterm.toml");

        if config_path.exists() {
            if let Ok(contents) = fs::read_to_string(&config_path) {
                if let Ok(config) = toml::from_str(&contents) {
                    return config;
                }
            }
        } else {
            let default_config = Self::default();
            if let Ok(toml_string) = toml::to_string_pretty(&default_config) {
                let _ = fs::write(&config_path, toml_string);
            }
        }
        Self::default()
    }
}
