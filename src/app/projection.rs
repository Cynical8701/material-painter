//! The projection tool: an image placed over the view, painted onto the mesh
//! where the brush passes, the way photos, decals and logos are put on a
//! model. The image stays fixed to the screen, so turning the camera moves
//! the model under it.
//!
//! Strokes go through the usual stamp pipeline with screen projection; the
//! engine takes their color (and coverage, from the image's alpha) from the
//! image where each texel lands on screen (`ImageProjection`).

use super::{PainterApp, ViewKind, labeled, section, theme};
use crate::engine::ImageProjection;
use crate::icons;
use eframe::egui;
use egui::{Color32, Rect, Stroke, pos2, vec2};
use glam::Vec2;
use std::sync::mpsc::{Receiver, channel};

/// Longest side images are scaled down to: plenty for painting, and it keeps
/// the upload and the overlay texture small.
const MAX_SIDE: u32 = 4096;

/// An image read from disk on a worker thread.
struct Decoded {
    name: String,
    size: [u32; 2],
    rgba: Vec<u8>,
}

/// The image over the view and where it sits.
pub(super) struct ProjImage {
    /// Distinguishes uploads to the engines.
    id: u64,
    name: String,
    size: [u32; 2],
    rgba: Vec<u8>,
    tex: egui::TextureHandle,
    /// Center as a fraction of the view's width and height.
    center: Vec2,
    /// Height as a fraction of the view's height.
    height: f32,
    /// Rotation on screen, radians (clockwise).
    angle: f32,
    /// How opaque the overlay is drawn (painting always uses the full image).
    opacity: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Move,
    Scale,
    Rotate,
}

#[derive(Clone, Copy)]
struct Drag {
    view: ViewKind,
    mode: DragMode,
    start: egui::Pos2,
    center: Vec2,
    height: f32,
    angle: f32,
}

#[derive(Default)]
pub(super) struct ProjectionState {
    pub(super) image: Option<ProjImage>,
    loading: Option<Receiver<Result<Decoded, String>>>,
    drag: Option<Drag>,
    next_id: u64,
}

impl ProjImage {
    /// Where the image sits in a view of `rect`: center and half size, in
    /// the same units as `rect`.
    fn placement(&self, rect: Rect) -> (egui::Pos2, egui::Vec2) {
        let center = rect.min + vec2(self.center.x * rect.width(), self.center.y * rect.height());
        let half_h = 0.5 * self.height * rect.height();
        let aspect = self.size[0] as f32 / self.size[1].max(1) as f32;
        (center, vec2(half_h * aspect, half_h))
    }

    /// Image fits inside the view, centered and upright.
    fn fit(&mut self, view_aspect: f32) {
        let aspect = self.size[0] as f32 / self.size[1].max(1) as f32;
        self.center = Vec2::splat(0.5);
        self.angle = 0.0;
        self.height = 0.9 * (view_aspect / aspect).min(1.0);
    }
}

fn decode(path: &std::path::Path) -> Result<Decoded, String> {
    let name = path.file_name().map_or("image".into(), |n| n.to_string_lossy().into_owned());
    let img = image::open(path).map_err(|e| format!("Can't open {name}: {e}"))?;
    let img = if img.width().max(img.height()) > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
    let rgba = img.to_rgba8();
    Ok(Decoded { name, size: [rgba.width(), rgba.height()], rgba: rgba.into_raw() })
}

impl PainterApp {
    fn view_aspect(&self) -> f32 {
        let [w, h] = self.views.view3d.size;
        if h == 0 { 1.0 } else { w as f32 / h as f32 }
    }

    /// Asks for an image file and reads it on a worker thread.
    fn pick_projection_image(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().add_filter("Image", &super::IMAGE_EXT).pick_file() else { return };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(decode(&path));
            ctx.request_repaint();
        });
        self.projection.loading = Some(rx);
    }

    /// Takes a finished image load, if any.
    pub(super) fn poll_projection(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.projection.loading else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.projection.loading = None;
        match result {
            Ok(d) => {
                let tex = ctx.load_texture(
                    "projection image",
                    egui::ColorImage::from_rgba_unmultiplied([d.size[0] as usize, d.size[1] as usize], &d.rgba),
                    egui::TextureOptions::LINEAR,
                );
                self.projection.next_id += 1;
                let mut img = ProjImage {
                    id: self.projection.next_id,
                    name: d.name,
                    size: d.size,
                    rgba: d.rgba,
                    tex,
                    center: Vec2::splat(0.5),
                    height: 0.9,
                    angle: 0.0,
                    opacity: self.projection.image.as_ref().map_or(0.5, |i| i.opacity),
                };
                img.fit(self.view_aspect());
                self.status = format!("Projecting {}. Paint over the mesh to put it on.", img.name);
                self.projection.image = Some(img);
            }
            Err(e) => self.status = e,
        }
    }

    /// Picks the projection tool with a generated test image over the view
    /// (offscreen UI tests).
    #[cfg(test)]
    pub(crate) fn test_projection(&mut self) {
        let (w, h) = (256u32, 128u32);
        let rgba = (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, 90, (i / w * 255 / h) as u8, 255]).collect();
        let (tx, rx) = channel();
        let _ = tx.send(Ok(Decoded { name: "test gradient.png".into(), size: [w, h], rgba }));
        self.projection.loading = Some(rx);
        self.tool = super::Tool::Projection;
    }

    /// Sets up the engine for a stroke in `kind`: the image painted through
    /// the view with the projection tool, the brush material otherwise.
    /// False (and a status message) when the projection tool has no image.
    pub(super) fn sync_projection(&mut self, kind: ViewKind) -> bool {
        if self.tool != super::Tool::Projection {
            self.engine.set_projection(None);
            return true;
        }
        let Some(img) = &self.projection.image else {
            self.status = "Load an image to project first.".into();
            return false;
        };
        let (size, view_proj, view2d) = match kind {
            ViewKind::V3d => {
                let [w, h] = self.views.view3d.size;
                (Vec2::new(w as f32, h as f32), self.cam.view_proj(w as f32 / h.max(1) as f32).to_cols_array_2d(), None)
            }
            ViewKind::V2d => {
                let [w, h] = self.views.view2d.size;
                (Vec2::new(w as f32, h as f32), glam::Mat4::IDENTITY.to_cols_array_2d(), Some([self.cam2d.zoom, self.cam2d.pan.x, self.cam2d.pan.y]))
            }
        };
        let (center, half) = img.placement(Rect::from_min_size(pos2(0.0, 0.0), vec2(size.x, size.y)));
        self.engine.set_projection_image(img.id, &img.rgba, img.size[0], img.size[1]);
        self.engine.set_projection(Some(ImageProjection {
            view_proj,
            view: size.to_array(),
            view2d,
            center: [center.x, center.y],
            half: [half.x, half.y],
            angle: img.angle,
        }));
        true
    }

    /// Hold S and drag to place the image: drag moves it, right-drag scales
    /// it, Shift+drag turns it. True while S is held (the brush rests).
    pub(super) fn projection_input(&mut self, ui: &egui::Ui, kind: ViewKind, rect: Rect, resp: &egui::Response) -> bool {
        let (held, hover, primary, secondary, down, shift) = ui.input(|i| {
            (
                i.key_down(egui::Key::S) && !i.modifiers.command && !i.modifiers.alt,
                i.pointer.hover_pos(),
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.pointer.any_down(),
                i.modifiers.shift,
            )
        });
        let held = held && !ui.ctx().egui_wants_keyboard_input();
        if !held || self.projection.image.is_none() {
            self.projection.drag = None;
            return held && self.projection.image.is_some();
        }
        let img = self.projection.image.as_mut().expect("checked above");
        if let Some(p) = hover.filter(|p| rect.contains(*p) && resp.hovered()) {
            if primary || secondary {
                let mode = match (secondary, shift) {
                    (true, _) => DragMode::Scale,
                    (false, true) => DragMode::Rotate,
                    (false, false) => DragMode::Move,
                };
                self.projection.drag = Some(Drag { view: kind, mode, start: p, center: img.center, height: img.height, angle: img.angle });
            }
        }
        let Some(d) = self.projection.drag.filter(|d| d.view == kind) else { return true };
        if !down {
            self.projection.drag = None;
            return true;
        }
        let Some(p) = hover else { return true };
        let delta = p - d.start;
        match d.mode {
            DragMode::Move => img.center = d.center + Vec2::new(delta.x / rect.width(), delta.y / rect.height()),
            DragMode::Scale => img.height = (d.height * ((delta.x - delta.y) * 0.005).exp()).clamp(0.02, 20.0),
            DragMode::Rotate => {
                let c = rect.min + vec2(d.center.x * rect.width(), d.center.y * rect.height());
                let a0 = (d.start - c).angle();
                let a1 = (p - c).angle();
                img.angle = d.angle + (a1 - a0);
            }
        }
        ui.ctx().set_cursor_icon(match d.mode {
            DragMode::Move => egui::CursorIcon::Grabbing,
            DragMode::Scale => egui::CursorIcon::ResizeNwSe,
            DragMode::Rotate => egui::CursorIcon::Alias,
        });
        true
    }

    /// The image over the view, while the projection tool is on.
    pub(super) fn draw_projection(&self, painter: &egui::Painter, rect: Rect) {
        let Some(img) = self.projection.image.as_ref().filter(|_| self.tool == super::Tool::Projection) else { return };
        let (center, half) = img.placement(rect);
        let (s, c) = img.angle.sin_cos();
        let corner = |x: f32, y: f32| center + vec2(x * half.x * c - y * half.y * s, x * half.x * s + y * half.y * c);
        let pts = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
        let uvs = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
        let tint = Color32::from_white_alpha((img.opacity.clamp(0.0, 1.0) * 255.0) as u8);
        let mut mesh = egui::Mesh::with_texture(img.tex.id());
        for (p, uv) in pts.iter().zip(uvs) {
            mesh.vertices.push(egui::epaint::Vertex { pos: *p, uv, color: tint });
        }
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        painter.add(mesh);
        let mut outline = pts.to_vec();
        outline.push(pts[0]);
        painter.add(egui::Shape::line(outline, Stroke::new(1.0, Color32::from_white_alpha(110))));
    }

    /// The projection tool's section of the properties.
    pub(super) fn projection_properties(&mut self, ui: &mut egui::Ui) {
        section(ui, "Projection");
        let loading = self.projection.loading.is_some();
        let aspect = self.view_aspect();
        let mut pick = false;
        match self.projection.image.as_mut() {
            None => {
                ui.label(egui::RichText::new("Place an image over the view, then paint over the mesh to put it on: photos, decals, logos.").small().color(theme::TEXT_DIM));
                ui.add_space(4.0);
                let text = if loading { "Loading…".to_string() } else { theme::with_icon(icons::IMAGES, "Load image…") };
                pick = ui.add_enabled(!loading, theme::primary(text)).clicked();
            }
            Some(img) => {
                labeled(ui, "Image", |ui| ui.label(egui::RichText::new(&img.name).color(theme::TEXT_BRIGHT)));
                labeled(ui, "", |ui| {
                    pick = ui.add_enabled(!loading, egui::Button::new(if loading { "Loading…" } else { "Replace…" })).clicked();
                    if ui.button("Fit to view").on_hover_text("Center the image in the view, upright").clicked() {
                        img.fit(aspect);
                    }
                });
                let mut op = img.opacity * 100.0;
                if labeled(ui, "Overlay", |ui| ui.add(theme::Slider(egui::Slider::new(&mut op, 10.0..=100.0).max_decimals(0).suffix("%")))).changed() {
                    img.opacity = op / 100.0;
                }
                labeled(ui, "Scale", |ui| ui.add(theme::Slider(egui::Slider::new(&mut img.height, 0.02..=20.0).logarithmic(true).max_decimals(2).suffix("x"))));
                let mut deg = img.angle.to_degrees();
                if labeled(ui, "Rotation", |ui| ui.add(theme::Slider(egui::Slider::new(&mut deg, -180.0..=180.0).max_decimals(0).suffix("°")))).changed() {
                    img.angle = deg.to_radians();
                }
                ui.label(egui::RichText::new("Hold S: drag to move · right-drag to scale · Shift+drag to rotate").small().color(theme::TEXT_DIM));
                ui.label(egui::RichText::new("Paints base color from the image; other enabled channels take the material below.").small().color(theme::TEXT_DIM));
            }
        }
        if pick {
            self.pick_projection_image(ui.ctx());
        }
    }
}
