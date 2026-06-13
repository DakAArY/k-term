use serde::{Deserialize, Serialize};

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
