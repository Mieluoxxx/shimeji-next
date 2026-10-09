//! wgpu 渲染：所有宠物窗口共享一个 Device 与纹理缓存，每个窗口一个 Surface。
//!
//! P0 结论：macOS Metal surface 只给 [Opaque, PostMultiplied]，所以纹理存 straight alpha，
//! 按 surface 能力选输出（PostMultiplied 直出，PreMultiplied 在 shader 里乘），不写死。
//! 只画一张铺满窗口的 quad，不需要混合；旧版用 ALPHA_BLENDING 会把 alpha 平方、边缘发暗。

use crate::assets::Sprite;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_straight(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}

@fragment
fn fs_premul(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#;

const QUAD_NORMAL: [f32; 16] = [
    -1.0, 1.0, 0.0, 0.0, //
    -1.0, -1.0, 0.0, 1.0, //
    1.0, 1.0, 1.0, 0.0, //
    1.0, -1.0, 1.0, 1.0, //
];

/// 朝右且没有 ImageRight 时镜像 uv，而不是再存一份翻转纹理
const QUAD_FLIPPED: [f32; 16] = [
    -1.0, 1.0, 1.0, 0.0, //
    -1.0, -1.0, 1.0, 1.0, //
    1.0, 1.0, 0.0, 0.0, //
    1.0, -1.0, 0.0, 1.0, //
];

pub struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    quads: [wgpu::Buffer; 2],
    pipelines: HashMap<(wgpu::TextureFormat, bool), wgpu::RenderPipeline>,
    textures: HashMap<(String, bool), wgpu::BindGroup>,
    warned_opaque: bool,
}

pub struct Surface {
    surface: wgpu::Surface<'static>,
    config: Option<wgpu::SurfaceConfiguration>,
    premultiply: bool,
}

fn quad(device: &wgpu::Device, data: &[f32; 16]) -> wgpu::Buffer {
    let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_ne_bytes()).collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("quad"),
        contents: &bytes,
        usage: wgpu::BufferUsages::VERTEX,
    })
}

impl Gpu {
    pub fn new(first: Arc<Window>) -> anyhow::Result<(Gpu, Surface)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(first)
            .map_err(|e| anyhow::anyhow!("创建 wgpu surface 失败: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| anyhow::anyhow!("没有可用的 GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("shimeji"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| anyhow::anyhow!("申请 GPU device 失败: {e}"))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shimeji-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprite"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        // 像素风皮肤，Retina 下整数倍放大，最近邻才不糊
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let quads = [quad(&device, &QUAD_NORMAL), quad(&device, &QUAD_FLIPPED)];
        let gpu = Gpu {
            instance,
            adapter,
            device,
            queue,
            shader,
            layout,
            pipeline_layout,
            sampler,
            quads,
            pipelines: HashMap::new(),
            textures: HashMap::new(),
            warned_opaque: false,
        };
        Ok((
            gpu,
            Surface {
                surface,
                config: None,
                premultiply: false,
            },
        ))
    }

    pub fn surface(&self, window: Arc<Window>) -> anyhow::Result<Surface> {
        let surface = self
            .instance
            .create_surface(window)
            .map_err(|e| anyhow::anyhow!("创建 wgpu surface 失败: {e}"))?;
        Ok(Surface {
            surface,
            config: None,
            premultiply: false,
        })
    }

    fn configure(&mut self, s: &mut Surface, width: u32, height: u32) {
        let caps = s.surface.get_capabilities(&self.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm))
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8Unorm);
        let modes = &caps.alpha_modes;
        let (alpha_mode, premultiply) = if modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            (wgpu::CompositeAlphaMode::PostMultiplied, false)
        } else if modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            (wgpu::CompositeAlphaMode::PreMultiplied, true)
        } else {
            if !self.warned_opaque {
                eprintln!("[render] 该平台 surface 不支持透明合成 {modes:?}，窗口会有底色");
                self.warned_opaque = true;
            }
            (modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto), false)
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        s.surface.configure(&self.device, &config);
        s.config = Some(config);
        s.premultiply = premultiply;
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat, premultiply: bool) -> &wgpu::RenderPipeline {
        self.pipelines.entry((format, premultiply)).or_insert_with(|| {
            self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sprite"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some(if premultiply { "fs_premul" } else { "fs_straight" }),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        })
    }

    fn texture(&mut self, sprite: &Sprite, srgb: bool) -> wgpu::BindGroup {
        let key = (sprite.key.clone(), srgb);
        if let Some(bg) = self.textures.get(&key) {
            return bg.clone();
        }
        let tex = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: sprite.width,
                    height: sprite.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // 与 surface 同为线性/同为 sRGB，最近邻采样下像素原样直通
                format: if srgb {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                },
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &sprite.rgba,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
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
        self.textures.insert(key, bg.clone());
        bg
    }

    pub fn draw(&mut self, s: &mut Surface, width: u32, height: u32, sprite: &Sprite, flipped: bool) {
        if width == 0 || height == 0 {
            return;
        }
        if s.config.as_ref().is_none_or(|c| c.width != width || c.height != height) {
            self.configure(s, width, height);
        }
        let Some(format) = s.config.as_ref().map(|c| c.format) else { return };
        let frame = match s.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.configure(s, width, height);
                match s.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(_) => return,
                }
            }
            Err(_) => return,
        };
        let bind_group = self.texture(sprite, format.is_srgb());
        let premultiply = s.premultiply;
        let pipeline = self.pipeline(format, premultiply).clone();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, self.quads[flipped as usize].slice(..));
            pass.draw(0..4, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}
