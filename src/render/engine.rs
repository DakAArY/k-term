use std::collections::HashMap;
use std::f32;
use std::sync::Arc;
use winit::window::Window;
use wgpu::util::DeviceExt;
use glyphon::{
    Attrs, Buffer as TextBuffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};

use crate::config::KtermConfig;
use crate::terminal::state::Terminal;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BgVertex {
    position: [f32; 2],
    color: [f32; 3],
}

impl BgVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BgVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

const BG_SHADER: &str = "
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec3<f32>,
}
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}
@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 0.0, 1.0);
    out.color = model.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
";

pub struct RenderState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    cache: Cache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    
    glyph_cache: HashMap<char, TextBuffer>,
    
    bg_pipeline: wgpu::RenderPipeline,
    bg_vertices: Vec<BgVertex>,
    pub bg_buffer: Option<wgpu::Buffer>,
    pub config_state: KtermConfig,
    pub exact_font_width: f32,
    pub exact_font_height: f32,
}

impl RenderState {
    pub async fn new(window: Arc<Window>, kterm_config: KtermConfig) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: None,
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
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

        let mut db = glyphon::fontdb::Database::new();
        db.load_system_fonts();
        db.load_font_data(include_bytes!("../../assets/FiraCodeNerdFont-Retina.ttf").to_vec());
        let mut font_system = FontSystem::new_with_locale_and_db("en-US".into(), db);

        let swash_cache = SwashCache::new();
        let font_size = kterm_config.font.size;
        let line_height = (font_size * 1.2).ceil();

        let mut measure_buf = TextBuffer::new(&mut font_system, Metrics::new(font_size, line_height));
        measure_buf.set_size(&mut font_system, None, None);
        measure_buf.set_text(
            &mut font_system,
            "W", 
            Attrs::new()
                .family(Family::Name(&kterm_config.font.family))
                .weight(Weight(400)),
            Shaping::Advanced,
        );
        
        let exact_font_width = measure_buf.layout_runs().next().map(|run| run.line_w).unwrap_or(font_size * 0.6);
        let exact_font_height = line_height;
        
        let cache = Cache::new(&device);
        let mut viewport = Viewport::new(&device, &cache);
        viewport.update(&queue, Resolution { width: size.width, height: size.height });

        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Background Shader"),
            source: wgpu::ShaderSource::Wgsl(BG_SHADER.into()),
        });
        
        let bg_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("BG Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        
        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BG Render Pipeline"),
            layout: Some(&bg_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[BgVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            font_system,
            swash_cache,
            viewport,
            cache,
            text_atlas,
            text_renderer,
            glyph_cache: HashMap::new(),
            bg_pipeline,
            bg_vertices: Vec::with_capacity(6000),
            bg_buffer: None,
            config_state: kterm_config,
            exact_font_width,
            exact_font_height,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            if new_size.width == self.config.width && new_size.height == self.config.height {
                return;
            }
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.viewport.update(
                &self.queue,
                Resolution { width: new_size.width, height: new_size.height },
            );
        }
    }

    pub fn render(&mut self, terminal: &mut Terminal) -> Result<(), wgpu::SurfaceError> {
        let win_w = self.size.width as f32;
        let win_h = self.size.height as f32;
        if win_w == 0.0 || win_h == 0.0 {
            return Ok(());
        }

        let font_width  = self.exact_font_width;
        let font_height = self.exact_font_height;
        let margin_left = 10.0_f32;
        let margin_top  = 10.0_f32;

        let cols       = terminal.cols;
        let rows       = terminal.rows;
        let default_bg = self.config_state.colors.background;
        let font_family = self.config_state.font.family.clone();
        let font_size = self.config_state.font.size;

        self.bg_vertices.clear();

        let mut missing_chars = Vec::new();
        for y in 0..rows {
            for x in 0..cols {
                let c = terminal.get_cell(x, y).c;
                if c != ' ' && c != '\0' && !self.glyph_cache.contains_key(&c) {
                    missing_chars.push(c);
                }
            }
        }
        
        missing_chars.sort_unstable();
        missing_chars.dedup();

        for c in missing_chars {
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(font_size, font_height));
            buf.set_size(&mut self.font_system, None, None);
            buf.set_text(
                &mut self.font_system,
                &c.to_string(),
                Attrs::new()
                    .family(Family::Name(&font_family))
                    .weight(Weight(400)),
                Shaping::Advanced, 
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            self.glyph_cache.insert(c, buf);
        }

        let mut text_areas = Vec::new();

        for y in 0..rows {
            let row_top = margin_top + y as f32 * font_height;
            
            for x in 0..cols {
                let cell = terminal.get_cell(x, y);
                let is_cursor = terminal.is_cursor(x, y) && !terminal.hide_cursor;

                let mut fg = cell.fg;
                let mut bg = cell.bg;

                if is_cursor {
                    std::mem::swap(&mut fg, &mut bg);
                    if fg == bg {
                        fg = default_bg;
                        bg = self.config_state.colors.foreground;
                    }
                }

                if bg != default_bg || is_cursor {
                    let left   = (margin_left + x as f32 * font_width).floor();
                    let right  = (margin_left + (x + 1) as f32 * font_width).ceil();
                    let top    = row_top.floor();
                    let bottom = (row_top + font_height).ceil();

                    let ndc_l = (left   / win_w) * 2.0 - 1.0;
                    let ndc_r = (right  / win_w) * 2.0 - 1.0;
                    let ndc_t = 1.0 - (top    / win_h) * 2.0;
                    let ndc_b = 1.0 - (bottom / win_h) * 2.0;

                    let c_color = [
                        bg[0] as f32 / 255.0,
                        bg[1] as f32 / 255.0,
                        bg[2] as f32 / 255.0,
                    ];
                    
                    self.bg_vertices.extend_from_slice(&[
                        BgVertex { position: [ndc_l, ndc_t], color: c_color },
                        BgVertex { position: [ndc_l, ndc_b], color: c_color },
                        BgVertex { position: [ndc_r, ndc_t], color: c_color },
                        BgVertex { position: [ndc_r, ndc_t], color: c_color },
                        BgVertex { position: [ndc_l, ndc_b], color: c_color },
                        BgVertex { position: [ndc_r, ndc_b], color: c_color },
                    ]);
                }

                if cell.c != ' ' && cell.c != '\0' {
                    let left_f = margin_left + x as f32 * font_width;
                    let top_f = margin_top + y as f32 * font_height;
                    
                    if let Some(cached_buffer) = self.glyph_cache.get(&cell.c) {
                        text_areas.push(TextArea {
                            buffer: cached_buffer,
                            left: left_f,
                            top: top_f,
                            scale: 1.0,
                            bounds: TextBounds {
                                left: 0,
                                top: 0,
                                right: win_w as i32,
                                bottom: win_h as i32,
                            },
                            default_color: Color::rgb(fg[0], fg[1], fg[2]),
                            custom_glyphs: &[],
                        });
                    }
                }
            }
        }

        self.bg_buffer = if !self.bg_vertices.is_empty() {
            Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("BG Vertex Buffer"),
                contents: bytemuck::cast_slice(&self.bg_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }))
        } else {
            None
        };

        terminal.dirty = false;

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();

        let output = self.surface.get_current_texture()?;
        let view   = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
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
                        load: wgpu::LoadOp::Clear({
                            let bg = self.config_state.colors.background;
                            wgpu::Color {
                                r: bg[0] as f64 / 255.0,
                                g: bg[1] as f64 / 255.0,
                                b: bg[2] as f64 / 255.0,
                                a: 1.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some(bg_buf) = &self.bg_buffer {
                render_pass.set_pipeline(&self.bg_pipeline);
                render_pass.set_vertex_buffer(0, bg_buf.slice(..));
                render_pass.draw(0..self.bg_vertices.len() as u32, 0..1);
            }

            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut render_pass)
                .unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.text_atlas.trim();

        Ok(())
    }

    pub fn get_glyph_dimensions(&mut self, font_size: f32) -> (f32, f32) {
        let line_height = font_size * 1.2;
        let metrics = Metrics::new(font_size, line_height);
        let mut measure_buffer = TextBuffer::new(&mut self.font_system, metrics);

        measure_buffer.set_size(&mut self.font_system, None, None);
        measure_buffer.set_text(&mut self.font_system, "W", Attrs::new(), Shaping::Advanced);
        
        let width = measure_buffer.layout_runs().next().map(|run| run.line_w).unwrap_or(font_size * 0.6);

        (width, line_height)
    }
}
