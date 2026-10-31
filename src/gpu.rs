//! Small wgpu helpers shared by the engine and the viewport renderer.

use eframe::wgpu;

pub struct Tex {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// sRGB-decoding view, only for Rgba8Unorm textures holding sRGB-encoded color.
    pub srgb_view: Option<wgpu::TextureView>,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub samples: u32,
}

pub const LAYER_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

impl Tex {
    pub fn new(
        device: &wgpu::Device,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Tex {
        Self::new_ms(device, label, width, height, format, usage, 1)
    }

    pub fn new_ms(
        device: &wgpu::Device,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        samples: u32,
    ) -> Tex {
        let srgb = format == wgpu::TextureFormat::Rgba8Unorm;
        let view_formats: &[wgpu::TextureFormat] =
            if srgb { &[wgpu::TextureFormat::Rgba8UnormSrgb] } else { &[] };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats,
        });
        let view = texture.create_view(&Default::default());
        let srgb_view = srgb.then(|| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                // sRGB formats can't be storage-bound; the linear view covers that.
                usage: Some(usage - wgpu::TextureUsages::STORAGE_BINDING),
                ..Default::default()
            })
        });
        Tex { texture, view, srgb_view, format, width, height, samples }
    }

    pub fn bytes(&self) -> u64 {
        self.width as u64 * self.height as u64 * bytes_per_texel(self.format) as u64 * self.samples as u64
    }

    pub fn copy_info(&self) -> wgpu::TexelCopyTextureInfo<'_> {
        wgpu::TexelCopyTextureInfo {
            texture: &self.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        }
    }

    pub fn extent(&self) -> wgpu::Extent3d {
        wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 }
    }

    pub fn srgb(&self) -> &wgpu::TextureView {
        self.srgb_view.as_ref().unwrap_or(&self.view)
    }
}

pub fn bytes_per_texel(format: wgpu::TextureFormat) -> u32 {
    use wgpu::TextureFormat as F;
    match format {
        F::R8Unorm => 1,
        F::Rg8Unorm | F::R16Float => 2,
        F::Rgba8Unorm | F::Rgba8UnormSrgb | F::Rg16Float | F::R32Float | F::Depth32Float => 4,
        F::Rgba16Float => 8,
        F::Rgba32Float => 16,
        _ => 4,
    }
}

pub fn clear_texture(encoder: &mut wgpu::CommandEncoder, tex: &Tex, color: wgpu::Color) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear"),
        color_attachments: &[color_attachment(&tex.view, wgpu::LoadOp::Clear(color))],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

pub fn color_attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
    })
}

pub fn pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    label: &str,
    attachments: &[Option<wgpu::RenderPassColorAttachment<'_>>],
) -> wgpu::RenderPass<'a> {
    encoder
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime()
}

pub fn tex_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub fn uniform_entry(binding: u32, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

pub fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    let full = format!("{}\n{}", include_str!("shaders/common.wgsl"), src);
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(full.into()),
    })
}

pub fn layout(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    })
}

/// Fullscreen-triangle pipeline writing to the given targets.
pub fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    fs_entry: &str,
    layout: &wgpu::PipelineLayout,
    targets: &[Option<wgpu::ColorTargetState>],
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs_entry),
            compilation_options: Default::default(),
            targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

pub fn target(format: wgpu::TextureFormat) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })
}

pub fn linear_sampler(device: &wgpu::Device, address: wgpu::AddressMode) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("linear"),
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

pub fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<crate::mesh::Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}

/// GPU copy of the current mesh, shared by the engine (UV bakes) and the viewport.
pub struct GpuMesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
    pub edges: wgpu::Buffer,
    pub edge_count: u32,
    pub seams: wgpu::Buffer,
    pub seam_count: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, mesh: &crate::mesh::Mesh) -> GpuMesh {
        use wgpu::util::DeviceExt;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let edges = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh uv edges"),
            contents: bytemuck::cast_slice(&mesh.uv_edges),
            usage: wgpu::BufferUsages::INDEX,
        });
        // (Never empty: a zero-size index buffer can't be bound.)
        let seam_data: &[u32] = if mesh.seam_edges.is_empty() { &[0, 0] } else { &mesh.seam_edges };
        let seams = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh uv seams"),
            contents: bytemuck::cast_slice(seam_data),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh {
            vertices,
            indices,
            index_count: mesh.indices.len() as u32,
            edges,
            edge_count: mesh.uv_edges.len() as u32,
            seams,
            seam_count: mesh.seam_edges.len() as u32,
        }
    }
}

/// Blocking texture readback, used for export (an explicit user action).
pub fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, tex: &Tex) -> Vec<u8> {
    let bpp = bytes_per_texel(tex.format);
    let row = tex.width * bpp;
    let padded = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * tex.height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        tex.copy_info(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(tex.height),
            },
        },
        tex.extent(),
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("readback buffer is mapped");
    let mut out = Vec::with_capacity((row * tex.height) as usize);
    for y in 0..tex.height as usize {
        let start = y * padded as usize;
        out.extend_from_slice(&data[start..start + row as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}

pub fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let man = (h & 0x3ff) as f32;
    match exp {
        0 => sign * man * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}
