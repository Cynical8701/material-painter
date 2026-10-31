//! Progressive path-traced preview of the 3D view. Accumulates one sample
//! per pixel per frame and restarts whenever the camera, viewport, lighting
//! or the composited material changes. Exposure and post-processing apply
//! after accumulation, so changing them never restarts it.

use crate::bvh::Bvh;
use crate::engine::Engine;
use crate::environment::{Environment, RADIANCE_MIPS, env_sampler};
use crate::gpu::{self, Tex};
use crate::mesh::Mesh;
use crate::post::{HDR_FORMAT, PostFx, PostSettings};
use crate::viewport::{OrbitCam, ViewMode, ViewSettings};
use eframe::{egui, egui_wgpu, wgpu};
use glam::Vec3;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RtUniform {
    inv_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    frame: [f32; 4],
    light: [f32; 4],
    params: [f32; 4],
    env: [f32; 4],
    bg: [f32; 4],
    sun: [f32; 4],
    sun_c: [f32; 4],
}

pub struct RtSettings {
    pub enabled: bool,
    pub max_samples: u32,
    pub bounces: u32,
    pub ground: bool,
}

impl Default for RtSettings {
    fn default() -> Self {
        Self { enabled: false, max_samples: 512, bounces: 4, ground: true }
    }
}

struct Scene {
    nodes: wgpu::Buffer,
    tri_pos: wgpu::Buffer,
    tri_nrm: wgpu::Buffer,
    tri_uv: wgpu::Buffer,
    tri_tan: wgpu::Buffer,
    ground_y: f32,
}

struct Target {
    accum: wgpu::Buffer,
    /// Scene-linear average of the samples so far.
    out: Tex,
    /// Post-processed image shown in the viewport.
    display: Tex,
    size: [u32; 2],
}

pub struct RayTracer {
    post: PostFx,
    env_sampler: wgpu::Sampler,
    /// Post settings the display image was last made with.
    shown_post: Option<(PostSettings, f32, bool)>,
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    scene: Option<Scene>,
    target: Option<Target>,
    pub egui_id: Option<egui::TextureId>,
    /// Samples accumulated for the current view.
    pub samples: u32,
    /// Everything that invalidates the accumulation, as raw bits.
    key: Vec<u32>,
}

fn tex(t: &Tex) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::TextureView(&t.view)
}

fn storage(device: &wgpu::Device, label: &str, data: &[[f32; 4]]) -> wgpu::Buffer {
    let fallback = [[0.0f32; 4]];
    let data = if data.is_empty() { &fallback[..] } else { data };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(data),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

impl RayTracer {
    pub fn new(rs: &egui_wgpu::RenderState) -> RayTracer {
        let device = &rs.device;
        let module = gpu::shader(device, "path trace", include_str!("shaders/pathtrace.wgsl"));
        let buffer = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path trace"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                buffer(1, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: HDR_FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                buffer(3, true),
                buffer(4, true),
                buffer(5, true),
                buffer(6, true),
                buffer(7, true),
                texture(8),
                texture(9),
                texture(10),
                texture(11),
                texture(12),
                wgpu::BindGroupLayoutEntry {
                    binding: 13,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(14),
                wgpu::BindGroupLayoutEntry {
                    binding: 15,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                buffer(16, true),
            ],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("path trace"),
            layout: Some(&gpu::layout(device, &bgl)),
            module: &module,
            entry_point: Some("cs_trace"),
            compilation_options: Default::default(),
            cache: None,
        });
        RayTracer {
            post: PostFx::new(rs, wgpu::TextureFormat::Rgba8UnormSrgb),
            env_sampler: env_sampler(device),
            shown_post: None,
            pipeline,
            bgl,
            sampler: gpu::linear_sampler(device, wgpu::AddressMode::ClampToEdge),
            scene: None,
            target: None,
            egui_id: None,
            samples: 0,
            key: Vec::new(),
        }
    }

    /// Uploads the paintable mesh with its BVH (built once per mesh load).
    pub fn set_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh, bvh: &Bvh) {
        let mut tri_pos = Vec::with_capacity(bvh.order.len() * 3);
        let mut tri_nrm = Vec::with_capacity(bvh.order.len() * 3);
        let mut tri_uv = Vec::with_capacity(bvh.order.len() * 3);
        let mut tri_tan = Vec::with_capacity(bvh.order.len() * 3);
        for &t in &bvh.order {
            for k in 0..3 {
                let v = &mesh.vertices[mesh.indices[t as usize * 3 + k] as usize];
                tri_pos.push([v.pos[0], v.pos[1], v.pos[2], 0.0]);
                tri_nrm.push([v.normal[0], v.normal[1], v.normal[2], 0.0]);
                tri_uv.push([v.uv[0], v.uv[1], 0.0, 0.0]);
                tri_tan.push(v.tangent);
            }
        }
        let mut nodes = Vec::with_capacity(bvh.nodes.len() * 2);
        for n in &bvh.nodes {
            nodes.push([n.min.x, n.min.y, n.min.z, f32::from_bits(n.first)]);
            nodes.push([n.max.x, n.max.y, n.max.z, f32::from_bits(n.count)]);
        }
        self.scene = Some(Scene {
            nodes: storage(device, "rt nodes", &nodes),
            tri_pos: storage(device, "rt positions", &tri_pos),
            tri_nrm: storage(device, "rt normals", &tri_nrm),
            tri_uv: storage(device, "rt uvs", &tri_uv),
            tri_tan: storage(device, "rt tangents", &tri_tan),
            ground_y: mesh.bounds_min.y - 1e-3,
        });
        self.key.clear();
    }

    fn ensure_target(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let device = &rs.device;
        let accum = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rt accumulation"),
            size: size[0] as u64 * size[1] as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let usage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let out = Tex::new(device, "rt output", size[0], size[1], HDR_FORMAT, usage);
        let display_usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        let display = Tex::new(device, "rt display", size[0], size[1], wgpu::TextureFormat::Rgba8UnormSrgb, display_usage);
        let mut renderer = rs.renderer.write();
        match self.egui_id {
            Some(id) => renderer.update_egui_texture_from_wgpu_texture(device, &display.view, wgpu::FilterMode::Linear, id),
            None => self.egui_id = Some(renderer.register_native_texture(device, &display.view, wgpu::FilterMode::Linear)),
        }
        self.target = Some(Target { accum, out, display, size });
        self.key.clear();
        self.shown_post = None;
    }

    pub fn converged(&self, settings: &RtSettings) -> bool {
        self.samples >= settings.max_samples
    }

    /// Adds one sample per pixel unless converged. Returns true if work was submitted.
    pub fn render(
        &mut self,
        rs: &egui_wgpu::RenderState,
        size: [u32; 2],
        engine: &Engine,
        env: &Environment,
        cam: &OrbitCam,
        view: &ViewSettings,
        settings: &RtSettings,
    ) -> bool {
        let size = [size[0].clamp(16, 8192), size[1].clamp(16, 8192)];
        self.ensure_target(rs, size);
        let Some(scene) = &self.scene else { return false };

        let vp = cam.view_proj(size[0] as f32 / size[1] as f32);
        let eye: Vec3 = cam.eye();
        let use_baked = view.use_baked_normal && engine.has_bake;
        let res = engine.res as f32;
        let key: Vec<u32> = vp
            .to_cols_array()
            .iter()
            .chain(&[
                view.env_rotation,
                view.env_intensity,
                view.background as u32 as f32,
                view.bg_blur,
                view.bg_intensity,
                view.bg_color[0],
                view.bg_color[1],
                view.bg_color[2],
                view.height_strength,
                if view.key.on { 1.0 } else { 0.0 },
                view.key.azimuth,
                view.key.elevation,
                view.key.intensity,
                view.key.color[0],
                view.key.color[1],
                view.key.color[2],
                if use_baked { 1.0 } else { 0.0 },
                settings.bounces as f32,
                if settings.ground { 1.0 } else { 0.0 },
                size[0] as f32,
                size[1] as f32,
            ])
            .map(|f| f.to_bits())
            .chain([engine.generation as u32, (engine.generation >> 32) as u32, env.id as u32, (env.id >> 32) as u32])
            .collect();
        if key != self.key {
            self.key = key;
            self.samples = 0;
        }
        let post = (view.post, view.exposure, view.mode != ViewMode::Material);
        if self.converged(settings) {
            if self.shown_post == Some(post) {
                return false;
            }
            // Only the grading changed: redo post on the finished image.
            let target = self.target.as_ref().unwrap();
            let mut encoder = rs.device.create_command_encoder(&Default::default());
            self.post.record(rs, &mut encoder, &target.out, &target.display.view, &post.0, post.1, post.2);
            rs.queue.submit([encoder.finish()]);
            self.shown_post = Some(post);
            return true;
        }

        let u = RtUniform {
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            cam_pos: eye.extend(1.0).to_array(),
            frame: [size[0] as f32, size[1] as f32, self.samples as f32, settings.bounces as f32],
            light: [view.env_rotation, 0.0, scene.ground_y, if settings.ground { 1.0 } else { 0.0 }],
            params: [if use_baked { 1.0 } else { 0.0 }, view.height_strength, 1.0 / res, res],
            env: [view.env_intensity, (RADIANCE_MIPS - 1) as f32, view.bg_blur, view.background as u32 as f32],
            bg: [view.bg_color[0], view.bg_color[1], view.bg_color[2], view.bg_intensity],
            sun: view.key.direction(view.env_rotation).extend(view.key.intensity).to_array(),
            sun_c: [view.key.color[0], view.key.color[1], view.key.color[2], if view.key.on { 1.0 } else { 0.0 }],
        };
        let device = &rs.device;
        let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt uniform"),
            contents: bytemuck::bytes_of(&u),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let target = self.target.as_ref().unwrap();
        let entries = [
            wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: target.accum.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: tex(&target.out) },
            wgpu::BindGroupEntry { binding: 3, resource: scene.nodes.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: scene.tri_pos.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: scene.tri_nrm.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: scene.tri_uv.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 7, resource: scene.tri_tan.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(engine.final_tex(0).srgb()) },
            wgpu::BindGroupEntry { binding: 9, resource: tex(engine.final_tex(1)) },
            wgpu::BindGroupEntry { binding: 10, resource: tex(engine.final_tex(2)) },
            wgpu::BindGroupEntry { binding: 11, resource: tex(engine.final_tex(3)) },
            wgpu::BindGroupEntry { binding: 12, resource: tex(engine.map_tex(3)) },
            wgpu::BindGroupEntry { binding: 13, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            wgpu::BindGroupEntry { binding: 14, resource: wgpu::BindingResource::TextureView(&env.radiance) },
            wgpu::BindGroupEntry { binding: 15, resource: wgpu::BindingResource::Sampler(&self.env_sampler) },
            wgpu::BindGroupEntry { binding: 16, resource: env.cdf.as_entire_binding() },
        ];
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("path trace"),
            layout: &self.bgl,
            entries: &entries,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
        self.post.record(rs, &mut encoder, &target.out, &target.display.view, &post.0, post.1, post.2);
        rs.queue.submit([encoder.finish()]);
        self.shown_post = Some(post);
        self.samples += 1;
        true
    }
}
