use std::sync::{Arc, RwLock};
use winit::window::Window;
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

use crate::state::buffer::TerminalState;

pub struct RenderState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,
    
    // Subsistemas de Glyphon
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,

    terminal_state: Arc<RwLock<TerminalState>>,

    text_buffer: Buffer,
}

impl<'a> RenderState<'a> {
    pub async fn new(window: Arc<Window>, terminal_state: Arc<RwLock<TerminalState>>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(Arc::clone(&window)).unwrap();

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: None,
            },
            None,
        ).await.unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| f.is_srgb())            
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Inicializamos Glyphon
        let mut font_system = FontSystem::new();
        
        // Cargamos tu Fira Code directamente a la base de datos de fuentes en memoria
        font_system.db_mut().load_font_data(
            include_bytes!("../../assets/FiraCode-VariableFont_wght.ttf").to_vec()
        );

        let swash_cache = SwashCache::new();
        let mut text_atlas = TextAtlas::new(&device, &queue, surface_format);
        let text_renderer = TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);

        let mut text_buffer = Buffer::new(&mut font_system, Metrics::new(16.0, 20.0));
        let size = window.inner_size();
        text_buffer.set_size(&mut font_system, size.width as f32, size.height as f32);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            terminal_state,
            text_buffer,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let mut is_dirty = false;
        let mut screen_text = String::new();

        {
            let mut current_state = self.terminal_state.write().unwrap();

            if current_state.dirty {
                for (y, row) in current_state.grid.iter().enumerate() {
                    for (x, cell) in row.iter().enumerate() {
                        if x == current_state.cursor_x && y == current_state.cursor_y {
                            screen_text.push('▒');
                        } else {
                            screen_text.push(cell.c);
                        }
                    }
                    screen_text.push('\n');
                }
                current_state.dirty = false;
                is_dirty = true;
            }
        }

        if is_dirty {
            self.text_buffer.set_text(
                &mut self.font_system,
                &screen_text,
                Attrs::new().family(Family::Name("Fira Code")).color(Color::rgb(255, 50, 50)),
                Shaping::Advanced,
            );
        }

        self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            Resolution { width: self.config.width, height: self.config.height },
            [TextArea {
                buffer: &self.text_buffer,
                left: 10.0,
                top: 10.0,
                scale: 1.0,
                bounds: TextBounds { 
                    left: 0,
                    top: 0,
                    right: self.size.width as i32,
                    bottom: self.size.height as i32,
                },
                default_color: Color::rgb(200, 200, 200),
            }],
            &mut self.swash_cache,
        ).unwrap(); 

        // 2. Iniciamos el proceso de dibujado a la pantalla
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05, g: 0.05, b: 0.05, a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Dibujamos el texto encima de nuestro fondo oscuro industrial
            self.text_renderer.render(&self.text_atlas, &mut render_pass).unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        // Limpiamos los buffers de texto para el siguiente frame
        self.text_atlas.trim();

        Ok(())
    }
}
