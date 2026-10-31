//! Image-based lighting from an equirectangular HDRI: a radiance map whose
//! mips are prefiltered for increasing roughness (reflections and background
//! blur), its diffuse irradiance as 9 spherical-harmonic coefficients, and a
//! luminance table the path tracer importance-samples.
//!
//! Decoding and the CPU-side tables run on a worker thread ([`decode`]); the
//! GPU prefilter runs once per environment ([`EnvBaker::upload`]).

use crate::gpu;
use eframe::{egui_wgpu, wgpu};
use glam::Vec3;
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};

/// Built-in environments: display name and file under `assets/hdri`
/// (`None` is the procedural studio). The files are CC0, from Poly Haven.
pub const BUILTIN: &[(&str, Option<&str>)] = &[
    ("Studio", None),
    ("Small Studio", Some("studio_small_09_1k.hdr")),
    ("Photo Studio", Some("brown_photostudio_02_1k.hdr")),
    ("Soft Studio", Some("christmas_photo_studio_07_1k.hdr")),
    ("Warehouse", Some("empty_warehouse_01_1k.hdr")),
    ("Interior", Some("lebombo_1k.hdr")),
    ("Partly Cloudy", Some("kloofendal_48d_partly_cloudy_puresky_1k.hdr")),
    ("Venice Sunset", Some("venice_sunset_1k.hdr")),
    ("Industrial Sunset", Some("industrial_sunset_02_puresky_1k.hdr")),
    ("Forest", Some("phalzer_forest_01_1k.hdr")),
    ("Night", Some("moonless_golf_1k.hdr")),
];

/// Radiance mips, from mirror (0) to fully rough (RADIANCE_MIPS - 1).
pub const RADIANCE_MIPS: u32 = 6;
/// Larger HDRIs are downsampled to this width on load.
const MAX_WIDTH: usize = 2048;
/// Resolution of the importance-sampling table.
const CDF_W: usize = 256;
const CDF_H: usize = 128;
/// Rgba16Float's largest finite value, with headroom.
const F16_MAX: f32 = 60000.0;

#[derive(Clone, PartialEq, Debug)]
pub enum EnvSource {
    Builtin(usize),
    File(PathBuf),
}

impl EnvSource {
    pub fn name(&self) -> String {
        match self {
            EnvSource::Builtin(i) => BUILTIN[*i].0.to_string(),
            EnvSource::File(p) => p.file_stem().map_or_else(|| p.display().to_string(), |s| s.to_string_lossy().into_owned()),
        }
    }

    /// A built-in by display name, or a file path.
    pub fn parse(s: &str) -> EnvSource {
        match BUILTIN.iter().position(|(n, _)| n.eq_ignore_ascii_case(s)) {
            Some(i) => EnvSource::Builtin(i),
            None => EnvSource::File(PathBuf::from(s)),
        }
    }
}

/// A decoded HDRI with its CPU-side lighting tables.
pub struct EnvImage {
    pub source: EnvSource,
    width: usize,
    height: usize,
    /// Linear RGB, row-major from the top (+Y) down.
    pixels: Vec<Vec3>,
    /// Irradiance SH, cosine lobe already convolved in (see `sh_basis`).
    sh: [[f32; 4]; 9],
    /// See [`Distribution2d`].
    cdf: Vec<f32>,
}

impl EnvImage {
    /// A small tonemapped preview (`w`×`h` RGBA8, sRGB): each pixel the
    /// average of the source pixels under it, exposed so the image's mean
    /// brightness lands mid-gray.
    pub fn thumbnail(&self, w: usize, h: usize) -> Vec<u8> {
        let (sw, sh) = (self.width, self.height);
        let mut lin = vec![Vec3::ZERO; w * h];
        for y in 0..h {
            for x in 0..w {
                let (x0, x1) = (x * sw / w, ((x + 1) * sw / w).max(x * sw / w + 1));
                let (y0, y1) = (y * sh / h, ((y + 1) * sh / h).max(y * sh / h + 1));
                let mut sum = Vec3::ZERO;
                for sy in y0..y1.min(sh) {
                    for sx in x0..x1.min(sw) {
                        sum += self.pixels[sy * sw + sx];
                    }
                }
                lin[y * w + x] = sum / ((y1 - y0) * (x1 - x0)).max(1) as f32;
            }
        }
        let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        let mean = lin.iter().map(|c| luma(*c)).sum::<f32>() / lin.len().max(1) as f32;
        let exposure = 0.35 / mean.max(1e-4);
        let encode = |v: f32| {
            let t = v / (1.0 + v);
            let s = if t <= 0.003_130_8 { t * 12.92 } else { 1.055 * t.powf(1.0 / 2.4) - 0.055 };
            (s.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        lin.iter().flat_map(|c| {
            let c = *c * exposure;
            [encode(c.x), encode(c.y), encode(c.z), 255]
        })
        .collect()
    }
}

/// Equirect convention shared with the shaders (`env_dir_to_uv`).
pub fn uv_to_dir(u: f32, v: f32) -> Vec3 {
    let phi = (u - 0.5) * TAU;
    let theta = v * PI;
    Vec3::new(theta.sin() * phi.sin(), theta.cos(), -theta.sin() * phi.cos())
}

fn sh_basis(d: Vec3) -> [f32; 9] {
    [
        0.282095,
        0.488603 * d.y,
        0.488603 * d.z,
        0.488603 * d.x,
        1.092548 * d.x * d.y,
        1.092548 * d.y * d.z,
        0.315392 * (3.0 * d.z * d.z - 1.0),
        1.092548 * d.x * d.z,
        0.546274 * (d.x * d.x - d.y * d.y),
    ]
}

fn assets_dir() -> Option<PathBuf> {
    asset_dir("hdri")
}

/// `assets/<name>`, looked up from the working directory and the
/// executable's folders (so it works from target/debug and installs).
pub(crate) fn asset_dir(name: &str) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::current_dir().into_iter().collect();
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().skip(1).map(Path::to_path_buf));
    }
    roots.into_iter().map(|r| r.join("assets").join(name)).find(|d| d.is_dir())
}

/// Built-in HDRIs whose files are present.
pub fn available_builtins() -> Vec<usize> {
    let dir = assets_dir();
    (0..BUILTIN.len())
        .filter(|&i| match BUILTIN[i].1 {
            None => true,
            Some(f) => dir.as_ref().is_some_and(|d| d.join(f).is_file()),
        })
        .collect()
}

/// Decodes an environment and builds its lighting tables. Slow-ish; call it
/// off the UI thread.
pub fn decode(source: &EnvSource) -> Result<EnvImage, String> {
    let (width, height, pixels) = match source {
        EnvSource::Builtin(i) => match BUILTIN[*i].1 {
            None => studio(1024),
            Some(file) => {
                let dir = assets_dir().ok_or("the assets/hdri folder is missing")?;
                load_file(&dir.join(file))?
            }
        },
        EnvSource::File(path) => load_file(path)?,
    };
    let sh = project_sh(width, height, &pixels);
    let cdf = Distribution2d::build(width, height, &pixels);
    Ok(EnvImage { source: source.clone(), width, height, pixels, sh, cdf })
}

fn load_file(path: &Path) -> Result<(usize, usize, Vec<Vec3>), String> {
    let img = image::open(path).map_err(|e| format!("can't read {}: {e}", path.display()))?.into_rgb32f();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w != 2 * h {
        return Err(format!("{} is {w}x{h}; an equirectangular HDRI is twice as wide as it is tall", path.display()));
    }
    let mut pixels: Vec<Vec3> = img.pixels().map(|p| Vec3::new(p[0], p[1], p[2]).max(Vec3::ZERO).min(Vec3::splat(F16_MAX))).collect();
    let (mut w, mut h) = (w, h);
    while w > MAX_WIDTH {
        pixels = (0..h / 2)
            .flat_map(|y| {
                let px = &pixels;
                (0..w / 2).map(move |x| {
                    let at = |dx: usize, dy: usize| px[(2 * y + dy) * w + 2 * x + dx];
                    (at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) * 0.25
                })
            })
            .collect();
        w /= 2;
        h /= 2;
    }
    Ok((w, h, pixels))
}

/// The procedural studio: sky gradient, a warm key light, a cool fill and a
/// softbox, as an HDRI so it lights exactly like the photographic ones.
fn studio(width: usize) -> (usize, usize, Vec<Vec3>) {
    let height = width / 2;
    let key = Vec3::new(0.55, 0.65, 0.52).normalize();
    let fill = Vec3::new(-0.7, 0.2, -0.6).normalize();
    let softbox = Vec3::new(-0.8, 0.25, 0.4).normalize();
    // Discs sized and bright enough to give the irradiance of the old
    // directional key (2.6) and fill (0.6) lights.
    let disc = |d: Vec3, axis: Vec3, radius: f32, irradiance: f32| {
        let cos_r = radius.cos();
        let t = d.dot(axis);
        if t <= cos_r {
            return 0.0;
        }
        let solid_angle = TAU * (1.0 - cos_r);
        // Soft rim so the disc filters cleanly.
        let edge = ((t - cos_r) / (1.0 - cos_r) * 4.0).min(1.0);
        irradiance / solid_angle * edge
    };
    let pixels = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let d = uv_to_dir((x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32);
                let zenith = Vec3::new(0.42, 0.50, 0.62);
                let horizon = Vec3::new(0.80, 0.78, 0.74);
                let ground = Vec3::new(0.20, 0.19, 0.18);
                let mut c = if d.y > 0.0 { horizon.lerp(zenith, d.y.powf(0.7)) } else { horizon.lerp(ground, (-d.y).powf(0.4)) };
                c += Vec3::new(0.9, 0.95, 1.0) * d.dot(softbox).max(0.0).powf(60.0) * 62.0 * 0.02;
                c += Vec3::new(1.0, 0.94, 0.86) * disc(d, key, 0.045, 2.6);
                c += Vec3::splat(1.0) * disc(d, fill, 0.15, 0.6);
                c.min(Vec3::splat(F16_MAX))
            })
        })
        .collect();
    (width, height, pixels)
}

fn project_sh(width: usize, height: usize, pixels: &[Vec3]) -> [[f32; 4]; 9] {
    // Every 4th texel is plenty for the low-frequency irradiance.
    let step = (width / 256).max(1);
    let mut acc = [Vec3::ZERO; 9];
    for y in (0..height).step_by(step) {
        let v = (y as f32 + 0.5) / height as f32;
        let d_omega = (TAU * step as f32 / width as f32) * (PI * step as f32 / height as f32) * (v * PI).sin();
        for x in (0..width).step_by(step) {
            let d = uv_to_dir((x as f32 + 0.5) / width as f32, v);
            let l = pixels[y * width + x];
            for (a, b) in acc.iter_mut().zip(sh_basis(d)) {
                *a += l * b * d_omega;
            }
        }
    }
    // Convolve with the clamped cosine lobe (Ramamoorthi & Hanrahan).
    let band = [PI, 2.0 * PI / 3.0, 2.0 * PI / 3.0, 2.0 * PI / 3.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0];
    std::array::from_fn(|i| (acc[i] * band[i]).extend(0.0).to_array())
}

/// Piecewise-constant 2D distribution over the equirect (as in PBRT),
/// flattened for the path tracer:
/// `[total, marginal cdf (H + 1), per row: cdf (W + 1), per row: func (W)]`.
struct Distribution2d;

impl Distribution2d {
    fn build(width: usize, height: usize, pixels: &[Vec3]) -> Vec<f32> {
        let (w, h) = (CDF_W, CDF_H);
        let mut func = vec![0.0f32; w * h];
        for y in 0..h {
            let v = (y as f32 + 0.5) / h as f32;
            let sin_t = (v * PI).sin();
            for x in 0..w {
                // Box-average the source texels this cell covers.
                let (x0, x1) = (x * width / w, ((x + 1) * width / w).max(x * width / w + 1));
                let (y0, y1) = (y * height / h, ((y + 1) * height / h).max(y * height / h + 1));
                let mut sum = 0.0;
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        sum += pixels[sy * width + sx].dot(Vec3::new(0.2126, 0.7152, 0.0722));
                    }
                }
                let lum = sum / ((x1 - x0) * (y1 - y0)) as f32;
                // A floor keeps black regions reachable, so the estimator stays unbiased.
                func[y * w + x] = (lum + 1e-3) * sin_t;
            }
        }
        let mut row_cdf = vec![0.0f32; h * (w + 1)];
        let mut row_int = vec![0.0f32; h];
        for y in 0..h {
            let cdf = &mut row_cdf[y * (w + 1)..(y + 1) * (w + 1)];
            for x in 0..w {
                cdf[x + 1] = cdf[x] + func[y * w + x] / w as f32;
            }
            row_int[y] = cdf[w];
            for c in cdf.iter_mut() {
                *c /= row_int[y].max(1e-12);
            }
        }
        let mut marginal = vec![0.0f32; h + 1];
        for y in 0..h {
            marginal[y + 1] = marginal[y] + row_int[y] / h as f32;
        }
        let total = marginal[h];
        for m in marginal.iter_mut() {
            *m /= total.max(1e-12);
        }
        let mut out = Vec::with_capacity(1 + (h + 1) + h * (w + 1) + w * h);
        out.push(total);
        out.extend(marginal);
        out.extend(row_cdf);
        out.extend(func);
        out
    }
}

/// An environment ready to light with.
pub struct Environment {
    /// Unique per upload, so caches (path tracer accumulation) can tell
    /// environments apart.
    pub id: u64,
    pub source: EnvSource,
    /// Rgba16Float equirect, [`RADIANCE_MIPS`] mips prefiltered by roughness.
    pub radiance: wgpu::TextureView,
    /// Irradiance SH coefficients (uniform, 9 x vec4).
    pub sh: wgpu::Buffer,
    /// Importance-sampling table (storage, see [`Distribution2d`]).
    pub cdf: wgpu::Buffer,
}

/// Prefilters HDRIs on the GPU. Created once and reused for every load.
pub struct EnvBaker {
    downsample: wgpu::ComputePipeline,
    prefilter: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PrefilterU {
    /// Output width, height, roughness, source width.
    size: [f32; 4],
    /// Source mip count, sample count.
    extra: [f32; 4],
}

/// Sampler for equirect maps: wraps around horizontally, trilinear.
pub fn env_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("environment"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

fn f32_to_f16(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    // Round to nearest; a carry into the exponent is the correct result.
    let h = ((exp as u32) << 10) + ((mant + 0x1000) >> 13);
    sign | h.min(0x7bff) as u16
}

impl EnvBaker {
    pub fn new(device: &wgpu::Device) -> EnvBaker {
        let module = gpu::shader(device, "env prefilter", include_str!("shaders/env_prefilter.wgsl"));
        let compute = wgpu::ShaderStages::COMPUTE;
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("env prefilter"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: compute,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: compute,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: compute, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: compute,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let layout = gpu::layout(device, &bgl);
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        EnvBaker { downsample: pipeline("cs_downsample"), prefilter: pipeline("cs_prefilter"), bgl, sampler: env_sampler(device) }
    }

    pub fn upload(&self, rs: &egui_wgpu::RenderState, img: &EnvImage) -> Environment {
        let (device, queue) = (&rs.device, &rs.queue);
        let (w, h) = (img.width as u32, img.height as u32);
        let src_mips = h.ilog2() + 1;
        let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_DST;
        let texture = |label: &str, mips: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage,
                view_formats: &[],
            })
        };
        let mip_view = |t: &wgpu::Texture, mip: u32| {
            t.create_view(&wgpu::TextureViewDescriptor { base_mip_level: mip, mip_level_count: Some(1), ..Default::default() })
        };

        let source = texture("env source", src_mips);
        let data: Vec<u16> = img.pixels.iter().flat_map(|p| [p.x, p.y, p.z, 1.0]).map(f32_to_f16).collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &source, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 8), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let radiance = texture("env radiance", RADIANCE_MIPS);

        let uniform = |u: PrefilterU| {
            wgpu::util::DeviceExt::create_buffer_init(
                device,
                &wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM },
            )
        };
        let group = |buf: &wgpu::Buffer, src: &wgpu::TextureView, dst: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(src) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(dst) },
                ],
            })
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        // Box mip chain of the source, so wide GGX lobes sample it without noise.
        for mip in 1..src_mips {
            let (mw, mh) = ((w >> mip).max(1), (h >> mip).max(1));
            let buf = uniform(PrefilterU { size: [mw as f32, mh as f32, 0.0, w as f32], extra: [0.0; 4] });
            let bg = group(&buf, &mip_view(&source, mip - 1), &mip_view(&source, mip));
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.downsample);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(mw.div_ceil(8), mh.div_ceil(8), 1);
        }
        let all_mips = source.create_view(&Default::default());
        for mip in 0..RADIANCE_MIPS {
            let (mw, mh) = ((w >> mip).max(1), (h >> mip).max(1));
            let rough = mip as f32 / (RADIANCE_MIPS - 1) as f32;
            let buf = uniform(PrefilterU { size: [mw as f32, mh as f32, rough, w as f32], extra: [src_mips as f32, 192.0, 0.0, 0.0] });
            let bg = group(&buf, &all_mips, &mip_view(&radiance, mip));
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.prefilter);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(mw.div_ceil(8), mh.div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);

        let init = |label: &str, contents: &[u8], usage| {
            wgpu::util::DeviceExt::create_buffer_init(device, &wgpu::util::BufferInitDescriptor { label: Some(label), contents, usage })
        };
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Environment {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            source: img.source.clone(),
            radiance: radiance.create_view(&Default::default()),
            sh: init("env sh", bytemuck::cast_slice(&img.sh), wgpu::BufferUsages::UNIFORM),
            cdf: init("env cdf", bytemuck::cast_slice(&img.cdf), wgpu::BufferUsages::STORAGE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_round_trip() {
        for v in [0.0f32, 1.0, 0.5, 3.14159, 1000.0, 1e-5, 60000.0] {
            let back = gpu::f16_to_f32(f32_to_f16(v));
            assert!((back - v).abs() <= v * 1e-3 + 1e-7, "{v} -> {back}");
        }
    }

    #[test]
    fn studio_irradiance_is_brightest_toward_the_key() {
        let img = decode(&EnvSource::Builtin(0)).unwrap();
        let irradiance = |d: Vec3| img.sh.iter().zip(sh_basis(d)).map(|(c, b)| Vec3::from_slice(&c[..3]) * b).sum::<Vec3>();
        let key = Vec3::new(0.55, 0.65, 0.52).normalize();
        assert!(irradiance(key).x > irradiance(-key).x * 2.0);
    }

    #[test]
    fn cdf_ends_at_one() {
        let img = decode(&EnvSource::Builtin(0)).unwrap();
        let marginal_end = img.cdf[1 + CDF_H];
        assert!((marginal_end - 1.0).abs() < 1e-4);
    }
}
