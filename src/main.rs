mod app;
pub mod config;
pub mod pty;
pub mod render;
pub mod terminal;
pub mod input;

use anyhow::Result;

fn main() -> Result<()> {
    app::run()
}
