mod pty;
mod state;
mod render;

use anyhow::Result;
use state::buffer::TerminalState;
use vte::Parser;
use std::thread;
use std::sync::{Arc, RwLock};

use winit::{
    dpi::PhysicalSize, event::*, event_loop::EventLoop, window::WindowBuilder
};
use winit::keyboard::{Key, NamedKey};
use render::engine::RenderState;

fn main() -> Result<()> {
    println!("[K-Term] Levantando subsistemas...");
    
    let terminal_state = Arc::new(RwLock::new(TerminalState::new(80, 24)));

    let pty_process = pty::shell::spawn_interactive_shell()?;
    let mut writer = pty_process.writer;
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

    let mut render_state = pollster::block_on(RenderState::new(Arc::clone(&window), render_terminal_state));

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { 
                window_id,
                ref event,
            } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),

                    WindowEvent::Resized(physical_size) => {
                        render_state.resize(*physical_size);
                    }

                    WindowEvent::RedrawRequested => {
                        match render_state.render() {
                            Ok(_) => {}
                            Err(wgpu::SurfaceError::Lost) => render_state.resize(render_state.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(e) => eprintln!("Error en renderizado: {:?}", e),
                        }
                    }

                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        if key_event.state == ElementState::Pressed {
                            match &key_event.logical_key {
                                Key::Named(NamedKey::Enter) => {
                                    let _ = writer.write_all(b"\r");
                                }
                                Key::Named(NamedKey::Backspace) => {
                                    let _ = writer.write_all(b"\x7f");
                                }
                                _ => {
                                    if let Some(text) = &key_event.text {
                                        let _ = writer.write_all(text.as_bytes());
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
