mod pty;
mod state;
mod render;

use arboard::Clipboard;
use anyhow::Result;
use state::buffer::TerminalState;
use vte::Parser;
use std::thread;
use std::sync::{Arc, RwLock};

use winit::{
    event::*, event_loop::EventLoop, window::WindowBuilder
};
use winit::keyboard::{Key, NamedKey,ModifiersState};
use render::engine::RenderState;
use portable_pty::PtySize;

use crate::state::config::KtermConfig;

fn main() -> Result<()> {
    println!("[K-Term] Levantando subsistemas...");

    let config = KtermConfig::load();
    let def_fg = config.colors.foreground;
    let def_bg = config.colors.background;

    let font_width = config.font.size * 0.6;
    let font_height = config.font.size * 1.25;
    
    let terminal_state = Arc::new(RwLock::new(TerminalState::new(80, 24, def_fg, def_bg)));

    let pty_process = pty::shell::spawn_interactive_shell()?;
    let mut writer = pty_process.writer;
    let mut master = pty_process.master;
    let receiver = pty_process.receiver;

    let pty_state = Arc::clone(&terminal_state);

    thread::spawn(move || {
        let mut parser = Parser::new();
        for bytes in receiver {
            let mut state = pty_state.write().unwrap();
            for byte in bytes {
                parser.advance(&mut *state, byte);
            }
        }
    });

    let event_loop = EventLoop::new().unwrap();

    let window = Arc::new(WindowBuilder::new()
        .with_title("K-Term")
        .build(&event_loop)
        .unwrap());

    let render_terminal_state = Arc::clone(&terminal_state);

    let mut render_state = pollster::block_on(RenderState::new(Arc::clone(&window), Arc::clone(&terminal_state), config.clone()));

    let mut modifiers = ModifiersState::default();

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { 
                window_id,
                ref event,
            } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),

                    WindowEvent::MouseWheel { delta, .. } => {
                        let mut state = terminal_state.write().unwrap();
                        match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => {
                                state.scroll(-(*y as isize) * 3);
                            }
                            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                state.scroll(-(pos.y as isize / 15)); 
                            }
                        }
                    }

                    WindowEvent::Resized(physical_size) => {
                        render_state.resize(*physical_size);

                        let font_width = 9.6_f32;
                        let font_height = 20.0_f32;

                        let margin = 20.0_f32;

                        let new_cols = ((physical_size.width as f32 - margin) / font_width).max(1.0) as usize;
                        let new_rows = ((physical_size.height as f32 - margin) / font_height).max(1.0) as usize;

                        {
                            let mut state = terminal_state.write().unwrap();
                            state.resize(new_cols, new_rows);
                        }

                        let _ = master.resize(PtySize { 
                            rows: new_rows as u16,
                            cols: new_cols as u16,
                            pixel_width: physical_size.width as u16,
                            pixel_height: physical_size.height as u16,
                        });
                    }

                    WindowEvent::RedrawRequested => {
                        match render_state.render() {
                            Ok(_) => {}
                            Err(wgpu::SurfaceError::Lost) => render_state.resize(render_state.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(e) => eprintln!("Error en renderizado: {:?}", e),
                        }
                    }

                    WindowEvent::ModifiersChanged(new_modifiers) =>  {
                        modifiers = new_modifiers.state();
                    }

                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        if key_event.state == ElementState::Pressed {
                            match &key_event.logical_key {
                                Key::Named(NamedKey::Enter) => { let _ = writer.write_all(b"\r"); }
                                Key::Named(NamedKey::Backspace) => { let _ = writer.write_all(b"\x7f"); }
                                Key::Named(NamedKey::Tab) => { let _ = writer.write_all(b"\t"); }
                                Key::Named(NamedKey::Escape) => { let _ = writer.write_all(b"\x1b"); }
                                Key::Named(NamedKey::ArrowUp) => { let _ = writer.write_all(b"\x1b[A"); }
                                Key::Named(NamedKey::ArrowDown) => { let _ = writer.write_all(b"\x1b[B"); }
                                Key::Named(NamedKey::ArrowRight) => { let _ = writer.write_all(b"\x1b[C"); }
                                Key::Named(NamedKey::ArrowLeft) => { let _ = writer.write_all(b"\x1b[D"); }

                                Key::Character(c) => {
                                    let char_str = c.as_str();
                                    if modifiers.control_key() && modifiers.shift_key() && char_str.to_lowercase() == "v" {
                                        if let Ok(mut clipboard) = Clipboard::new() {
                                            if let Ok(text) = clipboard.get_text() {
                                                let _ = writer.write_all(text.as_bytes());
                                                let _ = writer.flush();
                                            }
                                        }
                                    }else if modifiers.control_key() && modifiers.shift_key() && char_str.to_lowercase() == "c" {
                                        println!("[K-Term] accion de copiar detectada")
                                    }else if modifiers.control_key() {
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
                    }
                    _ => {}
                }
            }

            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => {}

        }
    }).unwrap();

    Ok(())
}
