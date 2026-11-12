//! Drawing 3D triangles on the GPU, for `vyrn.draw`.
//!
//! The camera projection stays on the CPU (it is cheap: three corners per
//! triangle); what moves to the GPU is filling the triangles, with depth
//! testing and blending, which is where the time goes in big scenes.
//! Triangles are batched as they are drawn and sent in one go when the
//! pixels are next needed (a 2D shape, `show`, `save`, `get_pixel`), so
//! 2D drawn after the 3D (a HUD, a crosshair) lands on top as expected.
//!
//! A real GPU is used automatically. A software GPU (such as Mesa's
//! lavapipe) is not, since Vyrn's own renderer is as fast; `draw.use_gpu`
//! and the `VYRN_GPU` variable (`on`, `off`) choose explicitly.

use std::cell::RefCell;
use std::collections::HashMap;

/// One corner: position in clip space (x, y and depth already divided out
/// and multiplied back by `w`, the distance, so colours and texture
/// coordinates are interpolated with perspective), colour, and texture
/// coordinates (0 to 1, from the top left).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

/// An image to paint triangles with, as RGBA bytes, row by row from the top.
#[derive(Clone)]
pub struct TextureData {
    /// Unique to the image, to upload it once.
    pub id: u64,
    pub w: u32,
    pub h: u32,
    pub rgba: std::rc::Rc<Vec<u8>>,
}

/// Triangles that share a texture (or have none), three corners each.
pub struct Batch {
    pub texture: Option<TextureData>,
    pub verts: Vec<Vertex>,
}

const SHADER: &str = r#"
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@vertex
fn vs(@location(0) pos: vec4<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = pos;
    o.color = color;
    o.uv = uv;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    return i.color * textureSample(tex, samp, i.uv);
}
"#;

struct Target {
    w: u32,
    h: u32,
    color: wgpu::Texture,
    depth: wgpu::Texture,
    readback: wgpu::Buffer,
    padded_row: u32,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    targets: HashMap<u64, Target>,
    sampler: wgpu::Sampler,
    /// Uploaded textures by image id; key 0 is plain white, for triangles
    /// without one.
    textures: HashMap<u64, wgpu::BindGroup>,
    pub name: String,
}

/// Whether to use the GPU: unset is automatic.
#[derive(Clone, Copy, PartialEq)]
pub enum Choice {
    Auto,
    On,
    Off,
}

thread_local! {
    static CHOICE: RefCell<Choice> = RefCell::new(match std::env::var("VYRN_GPU").as_deref() {
        Ok("on" | "1" | "true") => Choice::On,
        Ok("off" | "0" | "false") => Choice::Off,
        _ => Choice::Auto,
    });
    /// The renderer, made the first time a 3D triangle is drawn; `None`
    /// inside when the software renderer is used.
    static RENDERER: RefCell<Option<Option<Renderer>>> = const { RefCell::new(None) };
}

pub fn choose(choice: Choice) {
    CHOICE.with(|c| *c.borrow_mut() = choice);
    RENDERER.with(|r| *r.borrow_mut() = None);
}

/// Whether 3D triangles go to the GPU, setting it up the first time.
pub fn active() -> bool {
    RENDERER.with(|r| {
        let mut r = r.borrow_mut();
        if r.is_none() {
            let choice = CHOICE.with(|c| *c.borrow());
            *r = Some(match choice {
                Choice::Off => None,
                _ => Renderer::new(choice == Choice::On).ok(),
            });
        }
        r.as_ref().is_some_and(|r| r.is_some())
    })
}

/// What draws 3D: "gpu: <adapter>" or "software".
pub fn description() -> String {
    active();
    RENDERER.with(|r| match r.borrow().as_ref() {
        Some(Some(r)) => format!("gpu: {}", r.name),
        _ => "software".to_string(),
    })
}

/// Fills the triangles of `batches` into the `w`-by-`h` pixels `buf`
/// (`0x00RRGGBB`) of the surface `id`, testing against that surface's
/// depth (cleared first when `clear_depth`).
pub fn render(
    id: u64,
    buf: &mut [u32],
    w: usize,
    h: usize,
    batches: &[Batch],
    clear_depth: bool,
) -> bool {
    RENDERER.with(|r| {
        let mut r = r.borrow_mut();
        match r.as_mut().and_then(|r| r.as_mut()) {
            Some(r) => {
                r.draw(id, buf, w as u32, h as u32, batches, clear_depth);
                true
            }
            None => false,
        }
    })
}

/// A surface is gone: free its GPU memory.
pub fn forget(id: u64) {
    let _ = RENDERER.try_with(|r| {
        if let Ok(mut r) = r.try_borrow_mut() {
            if let Some(Some(r)) = r.as_mut() {
                r.targets.remove(&id);
            }
        }
    });
}

impl Renderer {
    fn new(allow_software: bool) -> Result<Renderer, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU: {e}"))?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu && !allow_software {
            return Err("only a software GPU, which is no faster".into());
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vyrn draw"),
            ..Default::default()
        }))
        .map_err(|e| format!("cannot open {}: {e}", info.name))?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vyrn draw"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 32,
                shader_location: 2,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vyrn draw"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // Nearest-pixel sampling, repeating past the edges: the same as the
        // software renderer, so both draw the same picture.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let mut renderer = Renderer {
            device,
            queue,
            pipeline,
            targets: HashMap::new(),
            sampler,
            textures: HashMap::new(),
            name: format!("{} ({:?})", info.name, info.backend),
        };
        let white = TextureData {
            id: 0,
            w: 1,
            h: 1,
            rgba: std::rc::Rc::new(vec![255; 4]),
        };
        renderer.upload(&white);
        Ok(renderer)
    }

    /// Puts an image on the GPU (once), ready to paint with.
    fn upload(&mut self, t: &TextureData) {
        if self.textures.contains_key(&t.id) {
            return;
        }
        let size = wgpu::Extent3d {
            width: t.w,
            height: t.h,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &t.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(t.w * 4),
                rows_per_image: Some(t.h),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.textures.insert(t.id, group);
    }

    fn ensure_target(&mut self, id: u64, w: u32, h: u32) {
        let stale = self.targets.get(&id).is_none_or(|t| t.w != w || t.h != h);
        if stale {
            let size = wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            };
            let texture = |format, usage| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let color = texture(
                wgpu::TextureFormat::Bgra8Unorm,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
            );
            let depth = texture(
                wgpu::TextureFormat::Depth32Float,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            let padded_row = (w * 4).div_ceil(256) * 256;
            let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: u64::from(padded_row) * u64::from(h),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.targets.insert(
                id,
                Target {
                    w,
                    h,
                    color,
                    depth,
                    readback,
                    padded_row,
                },
            );
        }
    }

    fn draw(
        &mut self,
        id: u64,
        buf: &mut [u32],
        w: u32,
        h: u32,
        batches: &[Batch],
        clear_depth: bool,
    ) {
        for b in batches {
            if let Some(t) = &b.texture {
                self.upload(t);
            }
        }
        // Fresh depth for a new surface, as the software renderer starts.
        let new = !self.targets.contains_key(&id);
        self.ensure_target(id, w, h);
        let t = &self.targets[&id];
        let (color, depth, readback, padded_row) = (&t.color, &t.depth, &t.readback, t.padded_row);
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        // The pixels so far (0x00RRGGBB is B, G, R, 0 in memory).
        let bytes: Vec<u8> = buf.iter().flat_map(|p| p.to_le_bytes()).collect();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            size,
        );
        let mut vertex_bytes: Vec<u8> = Vec::new();
        let mut ranges = Vec::with_capacity(batches.len());
        let mut count = 0u32;
        for b in batches {
            for v in &b.verts {
                for f in v.pos.iter().chain(&v.color).chain(&v.uv) {
                    vertex_bytes.extend_from_slice(&f.to_le_bytes());
                }
            }
            let n = b.verts.len() as u32;
            ranges.push((b.texture.as_ref().map_or(0, |t| t.id), count..count + n));
            count += n;
        }
        if count == 0 {
            return;
        }
        let vertices = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: vertex_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vertices, 0, &vertex_bytes);
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: if clear_depth || new {
                            wgpu::LoadOp::Clear(1.0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            for (texture, range) in ranges {
                if let Some(group) = self
                    .textures
                    .get(&texture)
                    .or_else(|| self.textures.get(&0))
                {
                    pass.set_bind_group(0, group, &[]);
                    pass.draw(range, 0..1);
                }
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(h),
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        {
            let data = slice.get_mapped_range().expect("mapped after the wait");
            for y in 0..h as usize {
                let row = &data[y * padded_row as usize..][..w as usize * 4];
                for (x, px) in row.chunks_exact(4).enumerate() {
                    buf[y * w as usize + x] = u32::from_le_bytes([px[0], px[1], px[2], 0]);
                }
            }
        }
        readback.unmap();
    }
}
