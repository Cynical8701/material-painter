//! Picture cards for the Material Designer and Shader Maker start pages:
//! one graph holding every material preset and every node (templates, user
//! and built-in nodes fed with noise), rendered once into thumbnails.

use super::{PainterApp, theme};
use crate::graph::{self, Graph, Library, Runtime};
use eframe::egui;
use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Gallery {
    rt: Option<Runtime>,
    graph: Option<Graph>,
    lib: Option<Library>,
    /// Card key → the node whose render is its picture.
    keys: HashMap<String, u64>,
    /// What the gallery was built from (node sources), to rebuild on change.
    stamp: u64,
    maps_seen: u64,
    thumbs: HashMap<String, (egui::TextureId, usize)>,
}

/// Card keys.
pub(super) fn preset_key(i: usize) -> String {
    format!("preset/{i}")
}
pub(super) fn node_key(id: &str) -> String {
    format!("node/{id}")
}
pub(super) fn template_key(stem: &str) -> String {
    format!("node/template/{stem}")
}

impl PainterApp {
    /// Builds (when the node library changed) and renders the gallery, and
    /// keeps its egui textures current.
    pub(super) fn ensure_gallery(&mut self) {
        let lib = &self.graphs.lib;
        let stamp = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for d in &lib.defs {
                d.source.hash(&mut h);
            }
            h.finish()
        };
        let gal = &mut self.graphs.gallery;
        let rebuild = gal.graph.is_none() || gal.stamp != stamp;
        if rebuild {
            // Shading nodes shade the 3D view; they make no texture to show.
            let mut defs: Vec<_> = lib.defs.iter().filter(|d| !d.shading).cloned().collect();
            for (stem, _, src) in super::shader_ui::TEMPLATES {
                if let Ok(d) = graph::def::parse(&format!("template/{stem}"), src) {
                    defs.push(d);
                }
            }
            let mut g = Graph::new("Gallery");
            g.res = 128;
            let mut keys = HashMap::new();
            // Material presets, pasted side by side; their base color is the picture.
            let plib = Library { defs: defs.clone(), errors: Vec::new(), user_dir: None };
            for (k, p) in graph::presets::PRESETS.iter().enumerate() {
                let src = (p.build)();
                let base = src.outputs(&plib)[0];
                let mut map = HashMap::new();
                for n in &src.nodes {
                    map.insert(n.id, g.add(&n.def, n.pos));
                }
                for n in &src.nodes {
                    let id = map[&n.id];
                    let node = g.node_mut(id).expect("just added");
                    node.params = n.params.clone();
                    node.links = n.links.iter().map(|(i, from)| (i.clone(), map[from])).collect();
                }
                if let Some(b) = base {
                    keys.insert(preset_key(k), map[&b]);
                }
            }
            // Every node. Inputs get different patterns, so each card shows
            // what its node does: a colored one for color inputs, noise for
            // gray ones, then cells and a gradient for second and third inputs.
            let noise = g.add("noise", [0.0, 0.0]);
            let colored = g.add("gradient_map", [0.0, 0.0]);
            g.connect(noise, colored, "input");
            if let Some(n) = g.node_mut(colored) {
                n.params.insert("dark".into(), graph::Value::Color([0.08, 0.18, 0.28]));
                n.params.insert("mid".into(), graph::Value::Color([0.75, 0.38, 0.2]));
                n.params.insert("light".into(), graph::Value::Color([0.98, 0.86, 0.55]));
            }
            let cells = g.add("cells", [0.0, 0.0]);
            let ramp = g.add("gradient", [0.0, 0.0]);
            for d in &defs {
                let n = g.add(&d.id, [0.0, 0.0]);
                for (k, inp) in d.inputs.iter().enumerate() {
                    let src = match (k, inp.kind) {
                        (0, graph::Kind::Color) => colored,
                        (0, graph::Kind::Gray) => noise,
                        (1, _) => cells,
                        _ => ramp,
                    };
                    g.connect(src, n, &inp.name);
                }
                keys.insert(node_key(&d.id), n);
            }
            gal.graph = Some(g);
            gal.lib = Some(Library { defs, errors: Vec::new(), user_dir: None });
            gal.keys = keys;
            gal.stamp = stamp;
            gal.maps_seen = u64::MAX;
        }
        if rebuild || gal.maps_seen != self.engine.maps_version {
            let rt = gal.rt.get_or_insert_with(|| Runtime::new(&self.rs.device, &self.rs.queue));
            let engine = &self.engine;
            let maps = |m: usize| engine.mesh_map(m);
            rt.evaluate(&self.rs.device, &self.rs.queue, gal.graph.as_ref().expect("built"), gal.lib.as_ref().expect("built"), &maps, engine.maps_version);
            gal.maps_seen = engine.maps_version;
        }
        // egui textures for the thumbnails.
        let Some(rt) = &gal.rt else { return };
        let mut renderer = self.rs.renderer.write();
        for (key, node) in &gal.keys {
            let Some(t) = rt.thumb(*node) else { continue };
            let ptr = Arc::as_ptr(t) as usize;
            match gal.thumbs.get_mut(key) {
                Some((_, p)) if *p == ptr => {}
                Some((tex, p)) => {
                    renderer.update_egui_texture_from_wgpu_texture(&self.rs.device, &t.view, eframe::wgpu::FilterMode::Linear, *tex);
                    *p = ptr;
                }
                None => {
                    let tex = renderer.register_native_texture(&self.rs.device, &t.view, eframe::wgpu::FilterMode::Linear);
                    gal.thumbs.insert(key.clone(), (tex, ptr));
                }
            }
        }
    }

    pub(super) fn gallery_thumb(&self, key: &str) -> Option<egui::TextureId> {
        self.graphs.gallery.thumbs.get(key).map(|t| t.0)
    }
}

/// A clickable picture card: thumbnail, title and a line of description.
pub(super) fn card(ui: &mut egui::Ui, tex: Option<egui::TextureId>, title: &str, desc: &str, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size + 46.0), Sense::click());
    let p = ui.painter();
    let hover = resp.hovered();
    p.rect_filled(rect, 0.0, if hover { theme::WIDGET } else { theme::HEADER });
    p.rect_stroke(rect, 0.0, Stroke::new(if hover { 1.5 } else { 1.0 }, if hover { theme::ACCENT } else { theme::OUTLINE }), egui::StrokeKind::Inside);
    let img = Rect::from_min_size(rect.min + vec2(7.0, 7.0), vec2(size - 14.0, size - 14.0));
    match tex {
        Some(t) => {
            p.image(t, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.rect_filled(img, 0.0, Color32::from_gray(26));
            p.text(img.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(size * 0.3), Color32::from_gray(90));
        }
    }
    p.text(pos2(rect.min.x + 10.0, img.max.y + 6.0), egui::Align2::LEFT_TOP, title, egui::FontId::new(14.0, theme::heading_family()), Color32::WHITE);
    let galley = p.layout(desc.to_string(), egui::FontId::proportional(10.5), theme::TEXT_DIM, size - 16.0);
    p.galley(pos2(rect.min.x + 10.0, img.max.y + 25.0), galley, theme::TEXT_DIM);
    resp.on_hover_text(desc)
}

/// A section heading for start pages.
pub(super) fn heading(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.add_space(14.0);
    ui.label(egui::RichText::new(title).size(19.0).family(theme::heading_family()).color(Color32::WHITE));
    if !sub.is_empty() {
        ui.label(egui::RichText::new(sub).color(theme::TEXT_DIM));
    }
    ui.add_space(6.0);
}
