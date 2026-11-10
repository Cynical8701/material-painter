//! Painting the bake cage: a brush in the 3D view that pushes the cage out
//! (or in) per low-poly vertex, where the cage preview shows misses or
//! overlaps, and Auto-fix, which fits it from the probe's hits. The paint is
//! part of the bake settings (`BakeSettings::cage_paint`): the live preview
//! and the cage overlay follow every stroke.

use super::{PainterApp, ViewKind, icons, theme};
use crate::bake::{CAGE_SCALE, CagePaint};
use crate::cage::Reach;
use eframe::egui::{self, Color32, Stroke};
use glam::{Vec2, Vec3};
use std::sync::Arc;

/// A change the bake window asked for, applied once the window is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum CageAction {
    AutoFix,
    Reset,
}

pub(super) struct CageBrush {
    /// Dragging in the 3D view paints the cage (instead of the tool).
    pub on: bool,
    /// Radius in screen points.
    pub radius: f32,
    /// How fast a stroke raises or lowers the cage.
    pub strength: f32,
    pub pending: Option<CageAction>,
    /// Where the brush is drawn this frame: center and radius, in screen points.
    pub cursor: Option<(egui::Pos2, f32)>,
}

impl Default for CageBrush {
    fn default() -> Self {
        Self { on: false, radius: 40.0, strength: 1.0, pending: None, cursor: None }
    }
}

/// The cage paint controls of the bake window (no cage file loaded).
pub(super) fn cage_brush_ui(ui: &mut egui::Ui, b: &mut CageBrush, painted: bool, probed: bool) {
    ui.horizontal(|ui| {
        if ui
            .add(theme::selectable(b.on, theme::with_icon(icons::PAINT_BRUSH, "Paint cage")))
            .on_hover_text("Drag over the model in the 3D view to push the cage out where rays miss (red); Ctrl+drag pulls it in where another part bleeds in (magenta); Shift+drag puts it back to the front distance.")
            .clicked()
        {
            b.on = !b.on;
        }
        if ui
            .add_enabled(probed, egui::Button::new(theme::with_icon(icons::MAGIC_WAND, "Auto-fix")))
            .on_hover_text("Fits the cage to the high poly: out wherever a vertex's ray can't reach its surface, in wherever it reaches another part first. Vertices that are fine keep their distance.")
            .clicked()
        {
            b.pending = Some(CageAction::AutoFix);
        }
        if ui.add_enabled(painted, egui::Button::new("Reset")).on_hover_text("Back to the front and rear distances everywhere").clicked() {
            b.pending = Some(CageAction::Reset);
        }
    });
    if b.on {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Size").color(theme::TEXT_DIM));
            ui.add(theme::Slider(egui::Slider::new(&mut b.radius, 5.0..=300.0).max_decimals(0).suffix(" px")));
            ui.label(egui::RichText::new("Strength").color(theme::TEXT_DIM));
            ui.add(theme::Slider(egui::Slider::new(&mut b.strength, 0.1..=4.0).max_decimals(1)));
        });
        ui.label(
            egui::RichText::new("Drag: out · Ctrl+drag: in · Shift+drag: undoes the cage paint under the brush (back to the front distance). The cage colors update as you paint.")
                .small()
                .color(theme::TEXT_DIM),
        );
    }
}

impl PainterApp {
    /// Whether 3D view drags paint the cage.
    pub(super) fn cage_brush_active(&self) -> bool {
        self.cage_brush.on && self.show_bake && self.bake_settings.builds_cage()
    }

    /// Applies what the bake window asked for.
    pub(super) fn apply_cage_action(&mut self) {
        let Some(action) = self.cage_brush.pending.take() else { return };
        match action {
            CageAction::Reset => {
                self.bake_settings.cage_paint = None;
                self.status = "Cage back to the front and rear distances.".into();
            }
            CageAction::AutoFix => {
                let Some(probe) = self.cage_preview.probe.clone() else { return };
                match probe.auto_fix(&Reach::new(&self.bake_settings)) {
                    Some((scale, r)) => {
                        self.bake_settings.cage_paint = Some(Arc::new(CagePaint::new(scale)));
                        self.status = match (r.raised, r.lowered, r.unreachable) {
                            (0, 0, 0) => "The cage already fits: nothing to fix.".into(),
                            (up, down, 0) => format!("Cage fitted: {up} vertices pushed out, {down} pulled in."),
                            (up, down, far) => format!(
                                "Cage fitted: {up} vertices pushed out, {down} pulled in. {far} have no surface within reach; their texels take the nearest surface."
                            ),
                        };
                    }
                    None => self.status = "Auto-fix needs a high poly and no cage file.".into(),
                }
            }
        }
    }

    /// Paints the cage under the cursor while the brush is on. `cursor`: view
    /// pixels; `at_ui`: the same point in screen points. True when it took the input.
    pub(super) fn cage_brush_input(&mut self, ui: &egui::Ui, cursor: Option<Vec2>, at_ui: Option<egui::Pos2>, down: bool) -> bool {
        self.cage_brush.cursor = None;
        let Some(px) = cursor else { return false };
        let size = Vec2::new(self.views.view3d.size[0] as f32, self.views.view3d.size[1] as f32);
        let (o, d) = self.cam.ray(px, size);
        let Some(hit) = self.bvh.raycast(o, d) else { return false };
        if let Some(p) = at_ui {
            self.cage_brush.cursor = Some((p, self.cage_brush.radius));
        }
        ui.ctx().request_repaint();
        if !down {
            return false;
        }
        // The brush's world radius at the hit's depth.
        let eye = self.cam.eye();
        let fwd = (self.cam.target - eye).normalize();
        let px_world = 2.0 * (self.cam.fov_y * 0.5).tan() / size.y.max(1.0);
        let radius = self.cage_brush.radius * self.ppp * (hit.pos - eye).dot(fwd).max(1e-4) * px_world;
        let (ctrl, shift) = ui.input(|i| (i.modifiers.ctrl || i.modifiers.command, i.modifiers.shift));
        // Time-based, so the rate doesn't follow the frame rate.
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
        let rate = self.cage_brush.strength * dt * 2.5;
        let n = self.mesh.vertices.len();
        let mut scale: Vec<f32> =
            self.bake_settings.cage_paint.as_deref().filter(|p| p.fits(n)).map_or_else(|| vec![1.0; n], |p| p.scales().to_vec());
        let mut changed = false;
        for (v, s) in self.mesh.vertices.iter().zip(scale.iter_mut()) {
            let dist = Vec3::from_array(v.pos).distance(hit.pos);
            if dist >= radius {
                continue;
            }
            // Smooth falloff to the rim.
            let x = dist / radius;
            let w = 1.0 - x * x * (3.0 - 2.0 * x);
            let next = if shift {
                // Back to 1 in about a third of a second at the center, whatever the strength.
                *s + (1.0 - *s) * (dt * 8.0 * w).min(1.0)
            } else {
                *s * (if ctrl { -rate } else { rate } * w).exp()
            };
            let next = next.clamp(*CAGE_SCALE.start(), *CAGE_SCALE.end());
            if next != *s {
                *s = next;
                changed = true;
            }
        }
        if changed {
            self.bake_settings.cage_paint = Some(Arc::new(CagePaint::new(scale)));
        }
        true
    }

    /// Sets the bake's high poly and distances, opens the bake window and
    /// turns the cage brush on or off (offscreen tests).
    #[cfg(test)]
    pub(crate) fn test_cage_setup(&mut self, high: std::path::PathBuf, front: f32, brush: bool) {
        self.bake_settings.high_poly = Some(high);
        self.bake_settings.front = front;
        self.show_bake = true;
        self.cage_preview.show = true;
        self.cage_brush.on = brush;
    }

    /// The probe is in, and its share of missed sample points (offscreen tests).
    #[cfg(test)]
    pub(crate) fn test_cage_missed(&self) -> Option<f32> {
        self.cage_preview.stats.map(|s| s.missed)
    }

    /// The painted cage multipliers: least and most (offscreen tests).
    #[cfg(test)]
    pub(crate) fn test_cage_paint_range(&self) -> Option<(f32, f32)> {
        let p = self.bake_settings.cage_paint.as_deref()?;
        Some(p.scales().iter().fold((f32::MAX, f32::MIN), |(lo, hi), s| (lo.min(*s), hi.max(*s))))
    }

    /// Where the 3D view is on screen (offscreen tests).
    #[cfg(test)]
    pub(crate) fn test_view_rect(&self) -> Option<egui::Rect> {
        self.custom_ui.view_rect
    }

    /// The cage brush's outline over the 3D view.
    pub(super) fn draw_cage_brush(&self, painter: &egui::Painter, kind: ViewKind) {
        if kind != ViewKind::V3d || !self.cage_brush_active() {
            return;
        }
        if let Some((c, r)) = self.cage_brush.cursor {
            // The ring says what a drag does now: out, in (Ctrl) or reset (Shift).
            let mods = painter.ctx().input(|i| i.modifiers);
            let (color, label) = if mods.shift {
                (Color32::from_rgb(235, 235, 235), "Reset")
            } else if mods.ctrl || mods.command {
                (Color32::from_rgb(120, 170, 255), "In")
            } else {
                (Color32::from_rgb(255, 200, 80), "Out")
            };
            painter.circle_stroke(c, r, Stroke::new(2.0, Color32::from_black_alpha(140)));
            painter.circle_stroke(c, r, Stroke::new(1.0, color));
            let at = c + egui::vec2(0.0, r + 10.0);
            let font = egui::FontId::proportional(12.0);
            painter.text(at + egui::vec2(1.0, 1.0), egui::Align2::CENTER_CENTER, label, font.clone(), Color32::from_black_alpha(180));
            painter.text(at, egui::Align2::CENTER_CENTER, label, font, color);
        }
    }
}
