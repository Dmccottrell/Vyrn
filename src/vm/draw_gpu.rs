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

/// One corner: position in normalized device coordinates and colour.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 4],
}

const SHADER: &str = r#"
struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>) -> Out {
    var o: Out;
    o.pos = vec4<f32>(pos, 1.0);
    o.color = color;
    return o;
}

@fragment
fn fs(i: Out) -> @location(0) vec4<f32> {
    return i.color;
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

/// Fills `tris` (three vertices each) into the `w`-by-`h` pixels `buf`
/// (`0x00RRGGBB`) of the surface `id`, testing against that surface's
/// depth (cleared first when `clear_depth`).
pub fn render(
    id: u64,
    buf: &mut [u32],
    w: usize,
    h: usize,
    tris: &[Vertex],
    clear_depth: bool,
) -> bool {
    RENDERER.with(|r| {
        let mut r = r.borrow_mut();
        match r.as_mut().and_then(|r| r.as_mut()) {
            Some(r) => {
                r.draw(id, buf, w as u32, h as u32, tris, clear_depth);
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
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 12,
                shader_location: 1,
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
        Ok(Renderer {
            device,
            queue,
            pipeline,
            targets: HashMap::new(),
            name: format!("{} ({:?})", info.name, info.backend),
        })
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
        tris: &[Vertex],
        clear_depth: bool,
    ) {
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
        let vertex_bytes: Vec<u8> = tris
            .iter()
            .flat_map(|v| {
                v.pos
                    .iter()
                    .chain(v.color.iter())
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<u8>>()
            })
            .collect();
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
            pass.draw(0..tris.len() as u32, 0..1);
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
