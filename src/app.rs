use anyhow::Result;
use portable_pty::{MasterPty, PtySize};
use std::sync::Arc;
use std::thread;
use vte::Parser;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::config::KtermConfig;
use crate::input::{keyboard::handle_keypress, AppMode};
use crate::pty::shell::spawn_interactive_shell;
use crate::render::engine::RenderState;
use crate::terminal::state::Terminal;

#[derive(Debug)]
pub enum AppEvent {
    PtyData(Vec<u8>),
}

struct KTermApp {
    config: KtermConfig,
    terminal: Terminal,
    master_pty: Box<dyn MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    parser: Parser,
    app_mode: AppMode,
    modifiers: ModifiersState,
    window: Option<Arc<Window>>,
    render_state: Option<RenderState>,
}

impl ApplicationHandler<AppEvent> for KTermApp {
    
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::PtyData(bytes) => {
                for byte in bytes {
                    self.parser.advance(&mut self.terminal, byte);
                }
                self.terminal.dirty = true;
                
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw(); 
                }
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("k-term")
                .with_transparent(true);
            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
            self.window = Some(window.clone());

            let kterm_config = self.config.clone();
            let render_state = pollster::block_on(RenderState::new(window, kterm_config));
            self.render_state = Some(render_state);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(physical_size) => {
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                if let Some(render_state) = &mut self.render_state {
                    render_state.resize(physical_size);

                    let font_size = self.config.font.size;
                    let (glyph_width, glyph_height) = render_state.get_glyph_dimensions(font_size);

                    let new_cols = (physical_size.width as f32 / glyph_width).floor() as usize;
                    let new_rows = (physical_size.height as f32 / glyph_height).floor() as usize;

                    self.terminal.resize(new_cols, new_rows);
                    
                    let _ = self.master_pty.resize(portable_pty::PtySize {
                        cols: new_cols as u16,
                        rows: new_rows as u16,
                        pixel_width: physical_size.width as u16,
                        pixel_height: physical_size.height as u16,
                    });
                }

                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    handle_keypress(
                        &event,
                        &self.modifiers,
                        &mut self.terminal,
                        &mut self.writer,
                        &mut self.app_mode,
                    );
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::RedrawRequested => {
                if let Some(render_state) = &mut self.render_state {
                    match render_state.render(&mut self.terminal) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => render_state.resize(render_state.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn run() -> Result<()> {
    let config = KtermConfig::load();
    let def_fg = config.colors.foreground;
    let def_bg = config.colors.background;

    let terminal = Terminal::new(80, 24, def_fg, def_bg);

    let pty_process = spawn_interactive_shell()?;
    let writer = pty_process.writer;
    let master_pty = pty_process.master;
    let receiver = pty_process.receiver;

    let event_loop = EventLoop::<AppEvent>::with_user_event().build().unwrap();
    
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();

    thread::spawn(move || {
        for bytes in receiver {
            if proxy.send_event(AppEvent::PtyData(bytes)).is_err() {
                break;
            }
        }
    });

    let mut app = KTermApp {
        config,
        terminal,
        master_pty,
        writer,
        parser: Parser::new(),
        app_mode: AppMode::Terminal,
        modifiers: ModifiersState::default(),
        window: None,
        render_state: None,
    };

    event_loop.run_app(&mut app)?;

    Ok(())
}
