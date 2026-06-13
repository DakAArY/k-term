use arboard::Clipboard;
use std::io::Write;
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::terminal::state::Terminal;
use super::AppMode;

pub fn handle_keypress(
    key_event: &KeyEvent,
    modifiers: &ModifiersState,
    terminal: &mut Terminal,
    writer: &mut Box<dyn Write + Send>,
    mode: &mut AppMode,
) {
    if modifiers.control_key() && modifiers.shift_key() {
        if let Key::Character(c) = &key_event.logical_key {
            if c.as_str() == " " {
                *mode = match mode {
                    AppMode::Terminal => AppMode::Navigation,
                    AppMode::Navigation => AppMode::Terminal,
                };
                return;
            }
        }
    }

    match mode {
        AppMode::Terminal => handle_terminal_mode(key_event, modifiers, writer),
        AppMode::Navigation => handle_navigation_mode(key_event, modifiers, terminal),
    }
}

fn handle_terminal_mode(
    key_event: &KeyEvent,
    modifiers: &ModifiersState,
    writer: &mut Box<dyn Write + Send>,
) {
    match &key_event.logical_key {
        Key::Named(NamedKey::Enter) => { let _ = writer.write_all(b"\r"); }
        Key::Named(NamedKey::Backspace) => { let _ = writer.write_all(b"\x7f"); }
        Key::Named(NamedKey::Tab) => { let _ = writer.write_all(b"\t"); }
        Key::Named(NamedKey::Escape) => { let _ = writer.write_all(b"\x1b"); }
        Key::Named(NamedKey::ArrowUp) => { let _ = writer.write_all(b"\x1b[A"); }
        Key::Named(NamedKey::ArrowDown) => { let _ = writer.write_all(b"\x1b[B"); }
        Key::Named(NamedKey::ArrowRight) => { let _ = writer.write_all(b"\x1b[C"); }
        Key::Named(NamedKey::ArrowLeft) => { let _ = writer.write_all(b"\x1b[D"); }

        Key::Character(c) =>  {
            let char_str = c.as_str();
            if modifiers.control_key() && modifiers.shift_key() && char_str.to_lowercase() == "v" {
                if let Ok(mut clipboard) = Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let _ = writer.write_all(text.as_bytes());
                    }
                }
            } else if modifiers.control_key() && modifiers.shift_key() && char_str.to_lowercase() == "c" { 
            } else if modifiers.control_key() {
                if let Some(char_str) = c.as_str().chars().next() {
                    let ctr_char = (char_str.to_ascii_uppercase() as u8) & 0x1F;
                    if ctr_char > 0 && ctr_char <= 31 {
                        let _ = writer.write_all(&[ctr_char]);
                    }
                }
            } else if modifiers.alt_key() {
                let _ = writer.write_all(b"\x1b");
                let _ = writer.write_all(c.as_str().as_bytes());
            } else {
                let _ = writer.write_all(c.as_str().as_bytes());
            }
        }
        _ => {
            if let Some(text) = &key_event.text {
                if !modifiers.control_key() && !modifiers.alt_key() {
                    let _ = writer.write_all(text.as_bytes());
                }
            }
        }
    }
    let _ = writer.flush();
}

fn handle_navigation_mode(
    key_event: &KeyEvent,
    _modifiers: &ModifiersState,
    terminal: &mut Terminal,
) {
    if let Key::Character(c) = &key_event.logical_key {
        match c.as_str() {
            "j" => terminal.scroll(-1),
            "k" => terminal.scroll(1),
            "d" => terminal.scroll(-10),
            "u" => terminal.scroll(10),
            "G" => terminal.snap_to_bottom(),
            _ => {}
        }
    }
}
