use std::fs;

use serde::{Deserialize, Serialize};



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
            None => {
                eprintln!("[K-Term] No se pudo determinar el directorio de configuracion. Usando defaults");
                return Self::default();
            }
        };

        config_path.push("kterm");

        if !config_path.exists() {
            if let Err(e) = fs::create_dir_all(&config_path) {
                eprintln!("[K-Term] Error creando directorio de configuracion: {}", e);
            }
        }

        config_path.push("kterm.toml");

        if config_path.exists() {
            if let Ok(contents) = fs::read_to_string(&config_path) {
                match toml::from_str(&contents) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("[K-Term] Error parseando kterm.toml: {}. Usando defaults", e),
                }
            }
        } else {
            println!("[K-Term] Creando archivo de configuracion por defecto en {:?}", config_path);
            let default_config = Self::default();
            if let Ok(toml_string) = toml::to_string_pretty(&default_config) {
                if let Err(e) = fs::write(&config_path, toml_string) {
                    eprintln!("[K-Term] No se pudo escribir el archivo de configuracion por defecto: {}", e);
                }
            }
        }
        Self::default()
    }
}
