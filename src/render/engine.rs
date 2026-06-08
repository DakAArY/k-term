use std::{cell, sync::{Arc, RwLock}};
use winit::window::Window;
use wgpu::util::DeviceExt;
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Weight
};

use crate::state::{buffer::TerminalState, config::KtermConfig};

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

pub struct RenderState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,
    
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    terminal_state: Arc<RwLock<TerminalState>>,
    text_buffer: Buffer,
    screen_text: String,
    color_spans: Vec<(usize, usize, [u8; 3])>,

    bg_pipeline: wgpu::RenderPipeline,
    bg_vertices: Vec<BgVertex>,
    bg_buffer: Option<wgpu::Buffer>,
    config_state: KtermConfig,
}

impl<'a> RenderState<'a> {
    pub async fn new(window: Arc<Window>, terminal_state: Arc<RwLock<TerminalState>>, kterm_config: KtermConfig) -> Self {
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

        let mut db = glyphon::fontdb::Database::new();
        db.load_system_fonts();
        db.load_font_data(
            include_bytes!("../../assets/FiraCodeNerdFont-Retina.ttf").to_vec()
        );
        let mut font_system = FontSystem::new_with_locale_and_db("en-US".into(), db);

        let swash_cache = SwashCache::new();
        let mut text_atlas = TextAtlas::new(&device, &queue, surface_format);
        let text_renderer = TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);
        
        let font_size = kterm_config.font.size;
        let line_height = font_size * 1.25;

        let mut text_buffer = Buffer::new(&mut font_system, Metrics::new(font_size, line_height));
        let size = window.inner_size();
        text_buffer.set_size(&mut font_system, size.width as f32, size.height as f32);

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
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
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
            multiview: None
        });

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
            screen_text: String::with_capacity(15000),
            color_spans: Vec::with_capacity(1000),
            bg_pipeline,
            bg_vertices: Vec::with_capacity(6000),
            bg_buffer: None,
            config_state: kterm_config,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.text_buffer.set_size(
                &mut self.font_system,
                new_size.width as f32,
                new_size.height as f32,
            );
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let mut is_dirty = false;
        
        {
            let mut current_state = self.terminal_state.write().unwrap();

            if current_state.dirty {
                self.screen_text.clear();
                self.color_spans.clear();
                self.bg_vertices.clear();

                let screen = if current_state.use_alt_screen {
                    &current_state.alt
                } else {
                    &current_state.primary
                };

                let mut current_color = [220, 220, 220];
                let mut span_start = 0;
                let font_width = self.config_state.font.size * 0.6;
                let font_height = self.config_state.font.size * 1.25;
                let margin_left = 10.0_f32;
                let margin_top = 10.0_f32;
                let win_w = self.size.width as f32;
                let win_h = self.size.height as f32;
                let offset = screen.scroll_offset;
                let scrollback_len = screen.scrollback.len();
                let visible_rows = screen.grid.len();

                for y in 0..visible_rows {
                    let row = if offset > 0 && y < offset {
                        let sb_idx = scrollback_len.saturating_sub(offset).saturating_add(y);
                        &screen.scrollback[sb_idx]
                    }else {
                        &screen.grid[y.saturating_sub(offset)]
                    };

                    for (x, cell) in row.iter().enumerate() {
                        let is_cursor = offset == screen.cursor_x && y == screen.cursor_y;
                        let c = if is_cursor { '▒' } else { cell.c };

                        if cell. bg != [18, 18, 18] {
                            let left = margin_left + (x as f32 * font_width);
                            let right = left + font_width;
                            let top = margin_top + (y as f32 * font_height);
                            let bottom = top + font_height;

                            let ndc_l = (left / win_w) * 2.0 - 1.0;
                            let ndc_r = (right / win_w) * 2.0 - 1.0;
                            let ndc_t = 1.0 - (top / win_h) * 2.0;
                            let ndc_b = 1.0 - (bottom / win_h) * 2.0;

                            let color = [
                                cell.bg[0] as f32 / 255.0,
                                cell.bg[1] as f32 / 255.0,
                                cell.bg[2] as f32 / 255.0,
                            ];

                            self.bg_vertices.extend_from_slice(&[
                                BgVertex { position: [ndc_l, ndc_t], color }, // Top-Left
                                BgVertex { position: [ndc_l, ndc_b], color }, // Bottom-Left
                                BgVertex { position: [ndc_r, ndc_t], color }, // Top-Right
                                BgVertex { position: [ndc_r, ndc_t], color }, // Top-Right
                                BgVertex { position: [ndc_l, ndc_b], color }, // Bottom-Left
                                BgVertex { position: [ndc_r, ndc_b], color }, // Bottom-Right
                            ]);
                        }

                        if cell.fg != current_color {
                            if self.screen_text.len() > span_start {
                                self.color_spans.push((span_start, self.screen_text.len(), current_color));
                            }
                            current_color = cell.fg;
                            span_start = self.screen_text.len();
                        }
                        self.screen_text.push(c);
                    }
                    self.screen_text.push('\n');
                }

                if self.screen_text.len() > span_start {
                    self.color_spans.push((span_start, self.screen_text.len(), current_color));
                }

                current_state.dirty = false;
                is_dirty = true;
            }
        }

        if is_dirty {
            let font_family = self.config_state.font.family.clone();
            let rich_text = self.color_spans.iter().map(|&(start, end, color)| {
                (
                    &self.screen_text[start..end],
                    Attrs::new()
                        .family(Family::Name(&font_family))
                        .weight(Weight(450))
                        .color(Color::rgb(color[0], color[1], color[2]))
                )
            });

            self.text_buffer.set_rich_text(
                &mut self.font_system,
                rich_text,
                Shaping::Basic,
            );

            if !self.bg_vertices.is_empty() {
                self.bg_buffer = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("BG Vertex Buffer"),
                    contents: bytemuck::cast_slice(&self.bg_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }));
            } else {
                self.bg_buffer = None;
            }
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
                            r: 0.07, g: 0.07, b: 0.07, a: 1.0,
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

            self.text_renderer.render(&self.text_atlas, &mut render_pass).unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.text_atlas.trim();

        Ok(())
    }
}
