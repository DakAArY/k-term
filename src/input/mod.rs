pub mod keyboard;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Terminal,
    Navigation,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Terminal
    }
}
