//! Offscreen previews: bake a mesh, render the real 3D viewport from several
//! cameras in several view modes and write the frames to
//! `target/bake_test/preview/*.png`. Nothing touches a window. Without
//! `MP_PREVIEW_MESH` the rounded cube is used.
//!
//! `MP_PREVIEW_MESH=path.fbx cargo test --bin material_painter preview -- --ignored --nocapture`

use crate::bake::BakeSettings;
use crate::bake::tests::{gpu_with_adapter, run_bake};
use crate::engine::{Engine, MAPS};
use crate::gpu::{GpuMesh, read_texture};
use crate::mesh::Mesh;
use crate::presets;
use crate::viewport::{OrbitCam, ViewMode, ViewRenderer, ViewSettings};
use eframe::{egui, egui_wgpu, wgpu};
use glam::Vec3;
use std::path::Path;
use std::sync::Arc;

fn render_state() -> egui_wgpu::RenderState {
    let (instance, adapter, device, queue) = gpu_with_adapter();
    let target_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let renderer = egui_wgpu::Renderer::new(&device, target_format, Default::default());
    egui_wgpu::RenderState {
        available_adapters: vec![adapter.clone()],
        adapter,
        instance,
        device,
        queue,
        target_format,
        renderer: Arc::new(egui::mutex::RwLock::new(renderer)),
    }
}

fn map_mode(name: &str) -> ViewMode {
    ViewMode::Map(MAPS.iter().position(|m| m.name == name).unwrap_or_else(|| panic!("no map {name}")))
}

#[test]
#[ignore]
fn preview_mesh() {
    let mesh = match std::env::var("MP_PREVIEW_MESH").as_deref() {
        Ok("ball") => crate::mesh::Primitive::MaterialBall.build(),
        Ok(path) => Mesh::load_file(Path::new(path)).unwrap_or_else(|e| panic!("{path}: {e}")),
        Err(_) => crate::mesh::Primitive::RoundedCube.build(),
    };
    let rs = render_state();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let res = 2048;
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), res);
    let (maps, report, _) = run_bake(&rs.device, &rs.queue, &mesh, &gpu_mesh, &BakeSettings::default(), res);
    engine.set_maps(maps, report);

    // A smart material that leans on the edge, cavity and AO maps.
    let smart_name = std::env::var("MP_PREVIEW_SMART").unwrap_or_else(|_| "Painted Steel Worn".into());
    let smart = presets::SMART_MATERIALS.iter().find(|s| s.name == smart_name).unwrap_or_else(|| panic!("no smart material {smart_name}"));
    crate::app::add_smart_material(&mut engine, &mut crate::undo::UndoStack::default(), smart, 0, None);
    engine.composite();

    // Cameras: the whole mesh, then close-ups of both ends of its long axis.
    let (lo, hi) = (mesh.bounds_min, mesh.bounds_max);
    let ext = hi - lo;
    let axis = if ext.x >= ext.y && ext.x >= ext.z {
        Vec3::X
    } else if ext.y >= ext.z {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let center = (lo + hi) * 0.5;
    let half = ext.dot(axis) * 0.5;
    let side_yaw = if axis == Vec3::X { 0.0 } else { std::f32::consts::FRAC_PI_2 };
    let cams = [
        ("full", OrbitCam { target: center, dist: 3.4, yaw: side_yaw + 0.3, pitch: 0.25, ..Default::default() }),
        ("end_a", OrbitCam { target: center + axis * half * 0.75, dist: 1.1, yaw: side_yaw + 0.6, pitch: 0.35, ..Default::default() }),
        ("end_b", OrbitCam { target: center - axis * half * 0.75, dist: 1.1, yaw: side_yaw - 0.5, pitch: 0.3, ..Default::default() }),
    ];
    let modes = [
        ("material", ViewMode::Material),
        ("ao", map_mode("Ambient Occlusion")),
        ("curvature", map_mode("Curvature")),
        ("edge", map_mode("Edge Distance")),
        ("cavity", map_mode("Cavity")),
        ("normal", map_mode("Normal")),
    ];

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/preview");
    std::fs::create_dir_all(&dir).unwrap();
    let mut views = ViewRenderer::new(&rs);
    let env = crate::environment::EnvBaker::new(&rs.device)
        .upload(&rs, &crate::environment::decode(&crate::environment::EnvSource::Builtin(0)).unwrap());
    let size = [960, 640];
    for (cname, cam) in &cams {
        for (mname, mode) in &modes {
            let settings = ViewSettings { mode: *mode, ..Default::default() };
            views.render_3d(&rs, size, &engine, &env, &gpu_mesh, cam, &settings, None);
            let img = views.view3d.image();
            let bytes = read_texture(&rs.device, &rs.queue, img);
            let file = dir.join(format!("{cname}_{mname}.png"));
            image::save_buffer(&file, &bytes, img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
        }
    }
    println!("  previews in {}", dir.display());
}


/// Every kind of edit, undone back to the start and redone to the end, must
/// reproduce the exact layer stack at each step.
#[test]
#[ignore]
fn undo_redo_roundtrip() {
    use crate::engine::{Blend, Content, FillParams, GenKind};
    use crate::undo::UndoStack;

    let rs = render_state();
    let mesh = crate::mesh::Primitive::RoundedCube.build();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), 256);
    let mut undo = UndoStack::default();

    // (ids, names, opacity, blend, has mask, has generator, is fill)
    let snapshot = |e: &Engine| -> Vec<(u64, String, u32, Blend, bool, bool, bool)> {
        e.layers
            .iter()
            .map(|l| {
                let m = l.mask.as_ref();
                (l.id, l.name.clone(), l.opacity.to_bits(), l.blend, m.is_some(), m.is_some_and(|m| m.generator.is_some()), matches!(l.content, Content::Fill(_)))
            })
            .collect()
    };
    let mut states = vec![snapshot(&engine)];
    let mut step = |e: &mut Engine, u: &mut UndoStack, f: &dyn Fn(&mut Engine, &mut UndoStack)| {
        f(e, u);
        e.commit_props(u);
        states.push(snapshot(e));
    };
    step(&mut engine, &mut undo, &|e, u| {
        e.add_paint_layer(0, None, None, u);
    });
    step(&mut engine, &mut undo, &|e, u| {
        e.add_fill_layer(1, None, "Fill".into(), FillParams::default(), u);
    });
    let smart = &presets::SMART_MATERIALS[0];
    step(&mut engine, &mut undo, &|e, u| {
        crate::app::add_smart_material(e, u, smart, 2, None);
    });
    step(&mut engine, &mut undo, &|e, _| {
        e.layers[0].opacity = 0.25;
        e.layers[0].blend = Blend::Multiply;
        e.layers[1].name = "Renamed".into();
    });
    step(&mut engine, &mut undo, &|e, u| {
        e.move_layer(0, 3, u);
    });
    step(&mut engine, &mut undo, &|e, u| e.add_mask(0, true, None, u));
    step(&mut engine, &mut undo, &|e, u| e.add_mask(1, true, Some(GenKind::ALL[0].default_generator()), u));
    step(&mut engine, &mut undo, &|e, u| e.remove_mask(0, u));
    step(&mut engine, &mut undo, &|e, u| {
        e.duplicate_layer(2, u);
    });
    step(&mut engine, &mut undo, &|e, u| e.clear_layer(4, u));
    step(&mut engine, &mut undo, &|e, u| e.delete_layer(1, u));
    step(&mut engine, &mut undo, &|e, u| e.delete_layer(0, u));

    let n = states.len() - 1;
    println!("  labels: {:?}", undo.undo.iter().map(|e| e.label.as_str()).collect::<Vec<_>>());
    assert_eq!(undo.undo.len(), n, "one history entry per step: {:?}", undo.undo.iter().map(|e| &e.label).collect::<Vec<_>>());
    for i in (0..n).rev() {
        engine.undo(&mut undo);
        assert_eq!(snapshot(&engine), states[i], "undo back to step {i}");
    }
    for (i, state) in states.iter().enumerate().skip(1) {
        engine.redo(&mut undo);
        assert_eq!(&snapshot(&engine), state, "redo to step {i}");
    }
    println!("  {} steps undone and redone: {:?}", n, undo.undo.iter().map(|e| e.label.as_str()).collect::<Vec<_>>());
}

/// Every built-in export template, in every format, from a baked and
/// painted rounded cube: files exist, decode, have the right size and
/// channel count, and padding fills the space between UV islands.
#[test]
#[ignore]
fn export_templates() {
    use crate::export::{self, Depth, Format, Padding, Settings};
    let mesh = Arc::new(crate::mesh::Primitive::RoundedCube.build());
    let rs = render_state();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let res = 512;
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), res);
    let (maps, report, _) = run_bake(&rs.device, &rs.queue, &mesh, &gpu_mesh, &BakeSettings::default(), res);
    engine.set_maps(maps, report);
    let smart = presets::SMART_MATERIALS.iter().find(|s| s.name == "Painted Steel Worn").unwrap();
    crate::app::add_smart_material(&mut engine, &mut crate::undo::UndoStack::default(), smart, 0, None);
    engine.composite();
    let snap = export::snapshot(&engine, &rs.device, &rs.queue, mesh.clone());

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/export");
    let _ = std::fs::remove_dir_all(&root);
    for (fi, format) in Format::ALL.into_iter().enumerate() {
        for (ti, t) in export::builtin_templates().iter().enumerate() {
            // Every template in PNG; the other formats with two templates.
            if format != Format::Png && ti > 1 {
                continue;
            }
            let settings = Settings {
                folder: root.join(format.ext()).join(format!("{ti}")),
                mesh_name: "Cube".into(),
                project_name: "Test".into(),
                format,
                depth: *format.depths().last().unwrap(),
                size: (fi % 2 == 1).then_some(256),
                padding: if ti == 1 { Padding::Transparent } else { Padding::Infinite },
                normal_strength: 1.0,
                use_baked_normal: true,
            };
            let files = export::export(&snap, t, &settings).unwrap_or_else(|e| panic!("{} {}: {e}", t.name, format.name()));
            assert_eq!(files.len(), t.maps.len());
            for (f, m) in files.iter().zip(&t.maps) {
                let img = image::open(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                let size = settings.size.unwrap_or(res);
                assert_eq!((img.width(), img.height()), (size, size), "{}", f.display());
                let want = match (m.layout, format) {
                    (export::Layout::Rgba, Format::Jpeg) => 3,
                    (export::Layout::Gray, Format::Exr) => 3,
                    (l, _) => l.channels(),
                };
                assert_eq!(img.color().channel_count() as usize, want, "{}", f.display());
            }
        }
    }
    // Infinite padding leaves no untouched black between islands of the
    // base color (the cube's islands are surrounded by empty UV space).
    let base = image::open(root.join("png/0/T_Cube_BaseColor.png")).unwrap().to_rgb8();
    let black = base.pixels().filter(|p| p.0 == [0, 0, 0]).count();
    assert!(black < (res * res / 200) as usize, "{black} black texels after padding");
    let _ = Depth::U8;
}

/// A stroke across the rounded cube with each brush projection (and once
/// with grain), then every fill projection: each must put paint on the mesh
/// and comparable amounts of it. Frames go to `target/bake_test/paint/`.
#[test]
#[ignore]
fn paint_projections() {
    use crate::app::{BrushProjection, resolve_stamps};
    use crate::engine::{BrushPaint, FillParams, MatValues, Projection, StampUniform};
    use crate::undo::{StrokeTarget, UndoStack};
    use glam::Vec2;

    let mesh = crate::mesh::Primitive::RoundedCube.build();
    let bvh = crate::bvh::Bvh::from_mesh(&mesh);
    let rs = render_state();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let res = 512;
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), res);
    let mut undo = UndoStack::default();
    engine.add_paint_layer(0, None, None, &mut undo);

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/paint");
    std::fs::create_dir_all(&dir).unwrap();
    let mut views = ViewRenderer::new(&rs);
    let env = crate::environment::EnvBaker::new(&rs.device)
        .upload(&rs, &crate::environment::decode(&crate::environment::EnvSource::Builtin(0)).unwrap());
    let cam = OrbitCam { target: (mesh.bounds_min + mesh.bounds_max) * 0.5, ..Default::default() };
    let size = [640u32, 480u32];
    let settings = ViewSettings::default();
    let save = |views: &ViewRenderer, name: &str| {
        let img = views.view3d.image();
        let bytes = read_texture(&rs.device, &rs.queue, img);
        image::save_buffer(dir.join(format!("{name}.png")), &bytes, img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
    };
    let red_texels = |engine: &Engine| {
        let bytes = read_texture(&rs.device, &rs.queue, engine.final_tex(0));
        bytes.chunks(4).filter(|p| p[0] > 150 && p[1] < 90).count()
    };
    let extent = (mesh.bounds_max - mesh.bounds_min).max_element();

    let mut counts = Vec::new();
    for (name, projection, grain) in [
        ("screen", BrushProjection::Screen, false),
        ("surface", BrushProjection::Surface, false),
        ("uv", BrushProjection::Uv, false),
        ("surface_grain", BrushProjection::Surface, true),
    ] {
        engine.clear_layer(0, &mut undo);
        if grain {
            let px = crate::textures::TEXTURES[0].render(256);
            engine.set_brush_grain(Some((&px, 256)));
        }
        views.render_3d(&rs, size, &engine, &env, &gpu_mesh, &cam, &settings, None);
        assert!(engine.begin_stroke(0, StrokeTarget::Content, [true, false, false, false], false));
        let radius = 28.0;
        let raw: Vec<([f32; 4], f32)> = (0..60).map(|i| ([180.0 + i as f32 * 5.0, 250.0, 1.0, 0.0], 1.0)).collect();
        let fsize = Vec2::new(size[0] as f32, size[1] as f32);
        let stamps = resolve_stamps(&mesh, &bvh, &cam, fsize, projection, 1, radius, &raw);
        assert!(!stamps.is_empty(), "{name}: no stamps hit the mesh");
        let su = StampUniform {
            view_proj: cam.view_proj(fsize.x / fsize.y).to_cols_array_2d(),
            cam_pos: cam.eye().extend(1.0).to_array(),
            view: [fsize.x, fsize.y, 0.0, 0.0],
            view2d: [0.0; 4],
            brush: [radius, 0.6, 1.0, 1.0],
            clip: [crate::viewport::NEAR, crate::viewport::FAR, 0.0, 0.0],
            proj: [projection as u32 as f32, 0.0, 85f32.to_radians().cos(), 0.0],
        };
        let paint = BrushPaint {
            mat: MatValues { enabled: [true, false, false, false], color: [0.9, 0.1, 0.05], ..Default::default() },
            mask_value: 1.0,
            opacity: 1.0,
            erase: false,
            grain: if grain { [1.0, 3.0 / extent, 0.0, 0.0] } else { [0.0; 4] },
        };
        engine.stroke_step(&stamps, su, views.view3d.depth_view(), &paint, None);
        engine.end_stroke();
        engine.composite();
        views.render_3d(&rs, size, &engine, &env, &gpu_mesh, &cam, &settings, None);
        save(&views, name);
        let n = red_texels(&engine);
        println!("  {name}: {} stamps, {n} painted texels", stamps.len());
        counts.push(n);
    }
    let (screen, surface, uv, grained) = (counts[0], counts[1], counts[2], counts[3]);
    assert!(screen > 500, "screen projection painted {screen} texels");
    // Same brush circle on screen: the surface and UV discs cover a
    // similar area (the surface one more, as it wraps the bevels).
    for (name, n) in [("surface", surface), ("uv", uv)] {
        assert!(n * 3 > screen && n < screen * 4, "{name} painted {n} texels vs screen {screen}");
    }
    assert!(grained < surface, "grain should hold back some paint ({grained} vs {surface})");

    // Fill projections with an image.
    engine.delete_layer(0, &mut undo);
    let png = {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&crate::textures::TEXTURES[0].render(256), 256, 256, image::ExtendedColorType::L8)
            .unwrap();
        png
    };
    let img = engine.decode_fill_image("grain.png".into(), Arc::new(png)).unwrap();
    for p in Projection::ALL {
        let fill = FillParams { projection: p, ..FillParams::default() };
        let idx = engine.add_fill_layer(0, None, p.name().into(), fill, &mut undo);
        engine.set_fill_image(idx, 0, Some(img.clone()), &mut undo);
        engine.composite();
        views.render_3d(&rs, size, &engine, &env, &gpu_mesh, &cam, &settings, None);
        save(&views, &format!("fill_{}", p.key()));
        let bytes = read_texture(&rs.device, &rs.queue, engine.final_tex(0));
        let lit = bytes.chunks(4).filter(|px| px[0] > 20).count();
        assert!(lit > 1000, "{}: image didn't show ({lit} texels)", p.name());
        engine.delete_layer(idx, &mut undo);
    }
    println!("  frames in {}", dir.display());
}

/// GPU time of one painting frame at 4K: stamping + applying a stroke step,
/// then recompositing a smart material stack with a paint layer on top.
/// `cargo test --bin material_painter perf_4k -- --ignored --nocapture`
#[test]
#[ignore]
fn perf_4k() {
    use crate::app::{BrushProjection, resolve_stamps};
    use crate::engine::{BrushPaint, MatValues, StampUniform};
    use crate::undo::{StrokeTarget, UndoStack};
    use glam::Vec2;
    use std::time::Instant;

    let mesh = crate::mesh::Primitive::RoundedCube.build();
    let bvh = crate::bvh::Bvh::from_mesh(&mesh);
    let rs = render_state();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let res = 4096;
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), res);
    let mut undo = UndoStack::default();
    let smart = presets::SMART_MATERIALS.iter().find(|s| s.name == "Painted Steel Worn").unwrap();
    crate::app::add_smart_material(&mut engine, &mut undo, smart, 0, None);
    let top = engine.layers.len();
    engine.add_paint_layer(top, None, None, &mut undo);
    // A second paint layer under the smart material: painting it keeps the
    // stack below in a cache, as more passes follow it.
    engine.add_paint_layer(0, None, None, &mut undo);
    let top = top + 1;
    println!("  {} layers", engine.layers.len());
    let wait = |rs: &egui_wgpu::RenderState| {
        rs.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    };
    engine.composite();
    wait(&rs);

    let mut views = ViewRenderer::new(&rs);
    let env = crate::environment::EnvBaker::new(&rs.device)
        .upload(&rs, &crate::environment::decode(&crate::environment::EnvSource::Builtin(0)).unwrap());
    let cam = OrbitCam { target: (mesh.bounds_min + mesh.bounds_max) * 0.5, ..Default::default() };
    let size = [1600u32, 1000u32];
    let settings = ViewSettings::default();
    views.render_3d(&rs, size, &engine, &env, &gpu_mesh, &cam, &settings, None);
    wait(&rs);

    let fsize = Vec2::new(size[0] as f32, size[1] as f32);
    let su = StampUniform {
        view_proj: cam.view_proj(fsize.x / fsize.y).to_cols_array_2d(),
        cam_pos: cam.eye().extend(1.0).to_array(),
        view: [fsize.x, fsize.y, 0.0, 0.0],
        view2d: [0.0; 4],
        brush: [35.0, 0.6, 1.0, 0.0],
        clip: [crate::viewport::NEAR, crate::viewport::FAR, 0.0, 0.0],
        proj: [0.0, 0.0, 85f32.to_radians().cos(), 0.0],
    };
    let paint = BrushPaint {
        mat: MatValues { enabled: [true, true, true, false], color: [0.9, 0.1, 0.05], ..Default::default() },
        mask_value: 1.0,
        opacity: 1.0,
        erase: false,
        grain: [0.0; 4],
    };
    for (name, layer) in [("top layer", top), ("bottom layer", 0)] {
    assert!(engine.begin_stroke(layer, StrokeTarget::Content, [true, true, true, false], false));
    let frames = 30;
    let (mut t_stroke, mut t_comp, mut t_view) = (0.0, 0.0, 0.0);
    for f in 0..frames {
        let raw: Vec<([f32; 4], f32)> = (0..6).map(|i| ([500.0 + (f * 6 + i) as f32 * 3.0, 500.0, 1.0, 0.0], 1.0)).collect();
        let stamps = resolve_stamps(&mesh, &bvh, &cam, fsize, BrushProjection::Screen, 0, 35.0, &raw);
        let t = Instant::now();
        let region = crate::app::stroke_region(&mesh, &bvh, &cam, fsize, BrushProjection::Screen, 0, 35.0, &stamps);
        engine.stroke_step(&stamps, su, views.view3d.depth_view(), &paint, region);
        wait(&rs);
        t_stroke += t.elapsed().as_secs_f64();
        let t = Instant::now();
        engine.composite();
        wait(&rs);
        t_comp += t.elapsed().as_secs_f64();
        let t = Instant::now();
        views.render_3d(&rs, size, &engine, &env, &gpu_mesh, &cam, &settings, None);
        wait(&rs);
        t_view += t.elapsed().as_secs_f64();
    }
    // Stroke frames recomposite from the painted layer up: the result must
    // match a full composite exactly.
    let partial: Vec<Vec<u8>> = (0..4).map(|c| read_texture(&rs.device, &rs.queue, engine.final_tex(c))).collect();
    engine.mark_dirty();
    engine.composite();
    for c in 0..4 {
        assert!(read_texture(&rs.device, &rs.queue, engine.final_tex(c)) == partial[c], "{name}: channel {c} differs from a full composite");
    }
    engine.end_stroke();
    let ms = |t: f64| t * 1000.0 / frames as f64;
    println!(
        "  {name}, per frame at {res}: stroke {:.2} ms, composite {:.2} ms, 3D view {:.2} ms",
        ms(t_stroke),
        ms(t_comp),
        ms(t_view)
    );
    }
}

/// Every built-in graph node compiles and renders, fed by noise where it has
/// inputs; the starter graph produces its outputs. Thumbnails go to
/// `target/bake_test/graph/`.
#[test]
#[ignore]
fn graph_nodes_render() {
    use crate::graph::{Graph, Library, Runtime, Value};

    let rs = render_state();
    let mesh = crate::mesh::Primitive::RoundedCube.build();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), 256);
    let lib = Library::load(None);
    let mut rt = Runtime::new(&rs.device, &rs.queue);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/graph");
    std::fs::create_dir_all(&dir).unwrap();
    let picture = dir.join("input.png");
    image::save_buffer(&picture, &crate::textures::TEXTURES[0].render(64), 64, 64, image::ExtendedColorType::L8).unwrap();
    let maps = |t: usize| engine.map_tex(t);

    let half = |b: &[u8]| {
        let h = u16::from_le_bytes([b[0], b[1]]);
        let (s, e, m) = ((h >> 15) as u32, ((h >> 10) & 31) as i32, (h & 1023) as f32);
        let v = if e == 0 { m / 1024.0 * 2f32.powi(-14) } else { (1.0 + m / 1024.0) * 2f32.powi(e - 15) };
        if s == 1 { -v } else { v }
    };
    let texture_defs: Vec<_> = lib.defs.iter().filter(|d| !d.shading).collect();
    let mut sheet = image::RgbaImage::new(128 * 6, 128 * texture_defs.len().div_ceil(6) as u32);
    for (k, def) in texture_defs.into_iter().enumerate() {
        let mut g = Graph::new(&def.name);
        g.res = 128;
        let noise = g.add("noise", [0.0; 2]);
        let n = g.add(&def.id, [0.0; 2]);
        for inp in &def.inputs {
            g.connect(noise, n, &inp.name);
        }
        if def.params.iter().any(|p| p.kind == crate::graph::ParamKind::Image) {
            g.node_mut(n).unwrap().params.insert("image".into(), Value::Image(Some(picture.clone())));
        }
        rt.evaluate(&rs.device, &rs.queue, &g, &lib, &maps, 0);
        assert!(rt.errors.is_empty(), "{}: {:?}", def.id, rt.errors);
        let tex = rt.output(n).unwrap_or_else(|| panic!("{} rendered nothing", def.id));
        let bytes = read_texture(&rs.device, &rs.queue, tex);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for (i, px) in bytes.chunks(8).enumerate() {
            let c = [0, 1, 2, 3].map(|j| half(&px[j * 2..]));
            assert!(c.iter().all(|v| v.is_finite()), "{}: non-finite output", def.id);
            lo = lo.min(c[0]);
            hi = hi.max(c[0]);
            let (x, y) = ((i % 128) as u32, (i / 128) as u32);
            let u8c = c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
            sheet.put_pixel((k % 6) as u32 * 128 + x, (k / 6) as u32 * 128 + y, image::Rgba([u8c[0], u8c[1], u8c[2], 255]));
        }
        println!("  {:<18} red range {lo:.3}..{hi:.3}", def.id);
    }
    sheet.save(dir.join("nodes.png")).unwrap();

    // The starter graph feeds base color and roughness.
    let g = Graph::starter("Starter");
    rt.evaluate(&rs.device, &rs.queue, &g, &lib, &maps, 0);
    assert!(rt.errors.is_empty(), "{:?}", rt.errors);
    let outs = g.outputs(&lib);
    assert!(rt.output(outs[0].unwrap()).is_some() && rt.output(outs[1].unwrap()).is_some());
    // Nothing changed: nothing re-renders.
    assert!(!rt.evaluate(&rs.device, &rs.queue, &g, &lib, &maps, 0));

    // Through a fill layer: the composite shows the graph's base color, and
    // a graph's output written into a mask varies across it.
    let mut engine = engine;
    let mut undo = crate::undo::UndoStack::default();
    let fill = crate::engine::FillParams { graph: Some(1), ..Default::default() };
    let idx = engine.add_fill_layer(0, None, "Graph".into(), fill, &mut undo);
    let out = |u: usize| outs[u].and_then(|n| rt.output(n).cloned());
    engine.set_graph_images(idx, [out(0), out(1), out(2), out(3)], "Starter");
    engine.composite();
    let color = read_texture(&rs.device, &rs.queue, engine.final_tex(0));
    let distinct: std::collections::HashSet<[u8; 3]> = color.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
    assert!(distinct.len() > 50, "base color shows {} distinct colors", distinct.len());
    engine.add_mask(idx, true, None, &mut undo);
    engine.set_graph_mask(idx, rt.output(outs[1].unwrap()).unwrap());
    let mask = read_texture(&rs.device, &rs.queue, engine.layers[idx].mask.as_ref().unwrap().paint.as_ref().unwrap());
    let (lo, hi) = mask.iter().fold((255u8, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!(hi > lo + 40, "mask range {lo}..{hi}");
    // Custom nodes: a valid one compiles; a broken one reports the
    // compiler's message instead of failing the device.
    let user = dir.join("user_nodes");
    let _ = std::fs::remove_dir_all(&user);
    let mut ulib = Library::load(Some(user.clone()));
    let id = ulib.save_user("tint", &crate::graph::template("Tint")).unwrap();
    assert!(rt.compile(&rs.device, ulib.find(&id).unwrap()).is_ok());
    let broken = crate::graph::template("Broken").replace("c.rgb * params.amount", "c.rgb * undefined_thing");
    let id = ulib.save_user("broken", &broken).unwrap();
    let err = rt.compile(&rs.device, ulib.find(&id).unwrap()).unwrap_err();
    assert!(err.contains("undefined_thing"), "{err}");
    // The location reads as a line of the user's file.
    let def = ulib.find(&id).unwrap();
    let want = broken.lines().position(|l| l.contains("undefined_thing")).unwrap() + 1;
    let mapped = def.map_error(&err);
    assert!(mapped.contains(&format!("line {want}:")), "expected line {want} in: {mapped}");
    println!("  broken node reports: {}", err.lines().find(|l| l.contains("undefined_thing")).unwrap_or(""));
    let img = engine.final_tex(0);
    image::save_buffer(dir.join("composited.png"), &color, img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
}

/// The ready-made material graphs render cleanly; their base colors go to
/// `target/bake_test/graph/presets.png`.
#[test]
#[ignore]
fn graph_presets_render() {
    use crate::graph::{Library, Runtime, presets::PRESETS};
    let rs = render_state();
    let mesh = crate::mesh::Primitive::RoundedCube.build();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), 256);
    let lib = Library::load(None);
    let maps = |t: usize| engine.map_tex(t);
    let mut sheet = image::RgbaImage::new(256 * PRESETS.len() as u32, 256);
    for (k, p) in PRESETS.iter().enumerate() {
        let mut g = (p.build)();
        g.res = 256;
        let mut rt = Runtime::new(&rs.device, &rs.queue);
        rt.evaluate(&rs.device, &rs.queue, &g, &lib, &maps, 0);
        assert!(rt.errors.is_empty(), "{}: {:?}", p.name, rt.errors);
        let base = g.outputs(&lib)[0].unwrap();
        let thumb = rt.thumb(base).unwrap();
        let bytes = read_texture(&rs.device, &rs.queue, thumb);
        let img = image::RgbaImage::from_raw(thumb.width, thumb.height, bytes).unwrap();
        image::imageops::overlay(&mut sheet, &img, k as i64 * 256, 0);
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/graph");
    std::fs::create_dir_all(&dir).unwrap();
    sheet.save(dir.join("presets.png")).unwrap();
}

/// Screenshots of the real UI, rendered offscreen by egui_kittest (no
/// window): every workspace, written to `target/bake_test/ui/`.
#[test]
#[ignore]
fn ui_screenshots() {
    use crate::app::{PainterApp, Workspace};
    use egui_kittest::kittest::Queryable;
    // The same device request as the app: the adapter's full limits.
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.device_descriptor = Arc::new(|adapter| wgpu::DeviceDescriptor {
        label: Some("ui test device"),
        required_limits: adapter.limits(),
        ..Default::default()
    });
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1600.0, 1300.0])
        .with_pixels_per_point(1.0)
        // A screenshot test, not an accessibility audit.
        .with_accessibility_check(false)
        .wgpu_setup(egui_wgpu::WgpuSetup::CreateNew(setup))
        .build_eframe(|cc| PainterApp::new(cc).expect("app starts"));
    harness.run_steps(8);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/ui");
    std::fs::create_dir_all(&dir).unwrap();
    // The welcome window opens at startup.
    harness.render().expect("renders").save(dir.join("welcome.png")).unwrap();
    // Its entries light up under the pointer.
    harness.get_by_label_contains("Open project").hover();
    harness.run_steps(2);
    harness.render().expect("renders").save(dir.join("welcome_hover.png")).unwrap();
    harness.state_mut().test_close_welcome();
    harness.run_steps(2);
    harness.state_mut().test_show_seams();
    harness.run_steps(3);
    harness.render().expect("renders").save(dir.join("seams.png")).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    for (name, ws) in [("paint", Workspace::Paint), ("designer", Workspace::Designer), ("shader", Workspace::ShaderMaker)] {
        harness.state_mut().set_workspace(ws);
        harness.run_steps(6);
        harness.render().expect("renders").save(dir.join(format!("{name}.png"))).unwrap();
    }
    // A graph open on the canvas, and the paint workspace showing its layer.
    harness.state_mut().set_workspace(Workspace::Designer);
    harness.state_mut().test_add_preset(1);
    harness.run_steps(10);
    harness.render().expect("renders").save(dir.join("designer_graph.png")).unwrap();
    // The export window keeps its size over many frames (it used to grow
    // to the full width of the screen).
    harness.state_mut().set_workspace(Workspace::Paint);
    harness.state_mut().test_open_export();
    harness.run_steps(30);
    harness.render().expect("renders").save(dir.join("export.png")).unwrap();
    // HDRI pictures in the Preview tab.
    let ctx = harness.ctx.clone();
    harness.state_mut().test_show_hdris(&ctx);
    harness.run_steps(4);
    harness.render().expect("renders").save(dir.join("hdri.png")).unwrap();
    // The asset library (network: waits for the catalog, then thumbnails).
    let start = std::time::Instant::now();
    while !harness.state_mut().test_open_library() && start.elapsed().as_secs() < 60 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 8 {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    harness.render().expect("renders").save(dir.join("library.png")).unwrap();
    // A downloaded material dropped on the mesh.
    harness.state_mut().test_add_downloaded();
    harness.run_steps(10);
    harness.render().expect("renders").save(dir.join("library_layer.png")).unwrap();
    // A smart material saved from the stack and applied again keeps its structure.
    let ctx = harness.ctx.clone();
    let [before, after] = harness.state_mut().test_smart_roundtrip(&ctx);
    println!("  smart material (layers, masks, generators, filters, images, folders): saved {before:?}, applied {after:?}");
    assert_eq!(before, after, "the smart material round-trips");
    harness.run_steps(10);
    harness.render().expect("renders").save(dir.join("smart_roundtrip.png")).unwrap();
    // The path tool: a stitch path in each view, previewed, then applied.
    for (name, in_3d) in [("path_3d", true), ("path_2d", false)] {
        harness.state_mut().test_path_setup(in_3d);
        harness.run_steps(4);
        harness.render().expect("renders").save(dir.join(format!("{name}_preview.png"))).unwrap();
        let status = harness.state_mut().test_path_apply();
        println!("  {name}: {status}");
        assert!(status.starts_with("Painted"), "{status}");
        harness.run_steps(6);
        harness.render().expect("renders").save(dir.join(format!("{name}.png"))).unwrap();
    }
    // A preview shader: Toon on the 3D view, and its graph in the Material Designer.
    harness.state_mut().set_workspace(Workspace::Paint);
    let toon = harness.state_mut().test_preview_preset(0);
    harness.run_steps(6);
    assert_eq!(harness.state_mut().test_preview_error(), None, "the Toon preview shader compiles in the app");
    harness.render().expect("renders").save(dir.join("preview_shader.png")).unwrap();
    harness.state_mut().test_open_graph(toon);
    harness.run_steps(8);
    harness.render().expect("renders").save(dir.join("preview_shader_graph.png")).unwrap();
}

/// Mask filters change what a layer covers: a white mask shows the layer
/// everywhere, Invert hides it, noise breakup eats into it, blur on a flat
/// mask changes nothing.
#[test]
#[ignore]
fn mask_filters_apply() {
    use crate::engine::{FillParams, FilterKind, MaskFilter, MatValues};
    let rs = render_state();
    let mesh = crate::mesh::Primitive::RoundedCube.build();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), 256);
    let mut undo = crate::undo::UndoStack::default();
    let fill = FillParams { mat: MatValues { enabled: [true, false, false, false], color: [1.0, 0.0, 0.0], ..Default::default() }, ..Default::default() };
    let idx = engine.add_fill_layer(0, None, "Red".into(), fill, &mut undo);
    engine.add_mask(idx, true, None, &mut undo);
    let red = |e: &mut Engine| {
        e.composite();
        let px = read_texture(&rs.device, &rs.queue, e.final_tex(0));
        let red = px.chunks(4).filter(|p| p[0] > 200 && p[1] < 60).count();
        red as f32 / (px.len() / 4) as f32
    };
    let full = red(&mut engine);
    assert!(full > 0.95, "white mask: {full}");
    let set = |e: &mut Engine, filters: Vec<MaskFilter>| {
        e.layers[idx].mask.as_mut().unwrap().filters = filters;
        e.mark_dirty();
    };
    set(&mut engine, vec![MaskFilter::new(FilterKind::Blur)]);
    let blurred = red(&mut engine);
    assert!(blurred > 0.95, "blurred white mask: {blurred}");
    set(&mut engine, vec![MaskFilter::new(FilterKind::Invert)]);
    let inverted = red(&mut engine);
    assert!(inverted < 0.01, "inverted: {inverted}");
    let mut noise = MaskFilter::new(FilterKind::Noise);
    noise.a = 1.0;
    set(&mut engine, vec![noise]);
    let broken = red(&mut engine);
    assert!(broken > 0.05 && broken < 0.9, "noise breakup: {broken}");
    // Switched off, the filter does nothing.
    let mut off = MaskFilter::new(FilterKind::Invert);
    off.enabled = false;
    set(&mut engine, vec![off]);
    assert!(red(&mut engine) > 0.95);
    println!("  red coverage: white {full:.2}, blur {blurred:.2}, invert {inverted:.2}, noise {broken:.2}");

    // Mask layers blend over the base mask.
    use crate::engine::{GenKind, MaskBlend, MaskLayer, MaskSource};
    let stack = |e: &mut Engine, layers: Vec<MaskLayer>| {
        let m = e.layers[idx].mask.as_mut().unwrap();
        m.filters.clear();
        m.layers = layers;
        e.mark_dirty();
    };
    let ml = |source, blend| MaskLayer { source, blend, opacity: 1.0, enabled: true };
    stack(&mut engine, vec![ml(MaskSource::Fill(0.0), MaskBlend::Multiply)]);
    assert!(red(&mut engine) < 0.01, "multiplied by black");
    stack(&mut engine, vec![ml(MaskSource::Fill(1.0), MaskBlend::Subtract)]);
    assert!(red(&mut engine) < 0.01, "white subtracted");
    stack(&mut engine, vec![ml(MaskSource::Fill(0.0), MaskBlend::Normal), ml(MaskSource::Generator(GenKind::Clouds.default_generator()), MaskBlend::Add)]);
    let clouds = red(&mut engine);
    assert!(clouds > 0.05 && clouds < 0.95, "clouds added over black: {clouds}");
    let mut off = ml(MaskSource::Fill(0.0), MaskBlend::Multiply);
    off.enabled = false;
    stack(&mut engine, vec![off]);
    assert!(red(&mut engine) > 0.95, "a switched-off mask layer does nothing");
    println!("  mask layers: clouds over black covers {clouds:.2}");
}

/// Every built-in shading node compiles into a preview shader on the GPU
/// (one small graph each), and every preset renders: images in
/// `target/bake_test/shading/`.
#[test]
#[ignore]
fn preview_shaders_compile_and_render() {
    use crate::graph::{Graph, Library, shading};
    let rs = render_state();
    let mesh = crate::mesh::Primitive::MaterialBall.build();
    let gpu_mesh = GpuMesh::new(&rs.device, &mesh);
    let res = 1024;
    let mut engine = Engine::new(&rs, &gpu_mesh, (mesh.bounds_min, mesh.bounds_max), res);
    let (maps, report, _) = run_bake(&rs.device, &rs.queue, &mesh, &gpu_mesh, &BakeSettings::default(), res);
    engine.set_maps(maps, report);
    let smart = presets::SMART_MATERIALS.iter().find(|s| s.name == "Painted Steel Worn").expect("smart material");
    crate::app::add_smart_material(&mut engine, &mut crate::undo::UndoStack::default(), smart, 0, None);
    engine.composite();
    let lib = Library::load(None);
    let mut views = ViewRenderer::new(&rs);

    // Each node alone, its output into the Shader Output.
    let mut failures = Vec::new();
    for def in lib.defs.iter().filter(|d| d.shading && d.id != shading::OUTPUT) {
        let mut g = Graph::new(&def.name);
        let n = g.add(&def.id, [0.0, 0.0]);
        let o = g.add(shading::OUTPUT, [200.0, 0.0]);
        g.connect(n, o, "color");
        let compiled = shading::compile(&g, &lib).unwrap_or_else(|e| panic!("{}: {e}", def.name));
        if let Err(e) = views.set_custom_shader(&rs.device, Some(&compiled.code)) {
            failures.push(format!("{}: {e}", def.name));
        }
    }
    assert!(failures.is_empty(), "shading nodes that don't compile:\n{}", failures.join("\n"));

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bake_test/shading");
    std::fs::create_dir_all(&dir).unwrap();
    let env = crate::environment::EnvBaker::new(&rs.device)
        .upload(&rs, &crate::environment::decode(&crate::environment::EnvSource::Builtin(0)).unwrap());
    let cam = OrbitCam { dist: 3.2, yaw: 0.5, pitch: 0.3, ..Default::default() };
    let settings = ViewSettings::default();
    let mut shots: Vec<(String, Option<Graph>)> = vec![("Default".into(), None)];
    shots.extend(shading::presets().into_iter().map(|g| (g.name.clone(), Some(g))));
    for (name, graph) in &shots {
        match graph {
            Some(g) => {
                let compiled = shading::compile(g, &lib).unwrap();
                views.set_custom_shader(&rs.device, Some(&compiled.code)).unwrap_or_else(|e| panic!("{name}: {e}"));
                views.set_custom_params(&rs.queue, &shading::param_values(&compiled, g, &lib));
            }
            None => views.set_custom_shader(&rs.device, None).unwrap(),
        }
        views.render_3d(&rs, [640, 640], &engine, &env, &gpu_mesh, &cam, &settings, None);
        let img = views.view3d.image();
        let bytes = read_texture(&rs.device, &rs.queue, img);
        let file = dir.join(format!("{}.png", name.replace(' ', "_").replace('+', "and")));
        image::save_buffer(&file, &bytes, img.width, img.height, image::ExtendedColorType::Rgba8).unwrap();
        println!("  {name} -> {}", file.display());
    }
}
