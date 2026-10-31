mod app;
mod bake;
mod brushes;
mod bvh;
mod cage;
mod engine;
mod environment;
mod export;
mod gpu;
mod graph;
mod icons;
mod import;
mod mcp;
mod mesh;
mod polyfill;
mod post;
mod presets;
#[cfg(test)]
mod preview_test;
mod raytrace;
mod select;
mod textures;
mod theme;
mod undo;
mod viewport;

use eframe::{egui_wgpu, wgpu};
use std::sync::Arc;

fn main() -> eframe::Result {
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Material Painter")
            .with_inner_size([1600.0, 960.0])
            .with_min_inner_size([1100.0, 680.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    // Request the adapter's full limits: high-poly bake sources need large
    // storage buffers, and the compositor binds many textures per pass.
    if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.power_preference = app::prefs::Preferences::power_preference();
        setup.device_descriptor = Arc::new(|adapter| wgpu::DeviceDescriptor {
            label: Some("material painter device"),
            required_limits: adapter.limits(),
            ..Default::default()
        });
    }
    eframe::run_native(
        "Material Painter",
        options,
        Box::new(|cc| Ok(Box::new(app::PainterApp::new(cc)?))),
    )
}
