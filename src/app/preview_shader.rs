//! Preview shaders: a shading graph (see `graph::shading`) that replaces the
//! 3D view's built-in look while you texture, e.g. toon, clay, unlit, or
//! the look of a target engine. Edited in the Material Designer like any
//! graph; chosen in the Preview panel; saved with the project.

use super::{PainterApp, Workspace, theme};
use crate::graph::shading;
use crate::icons;
use eframe::egui;

/// What the app keeps about the preview shader in use.
#[derive(Default)]
pub(super) struct PreviewShader {
    /// The shading graph in use (a graph id), or the built-in look.
    pub graph: Option<u64>,
    /// The code the view runs now (recompiled only when it changes).
    code: String,
    compiled: Option<shading::Compiled>,
    pub error: Option<String>,
}

impl PainterApp {
    /// Compiles the preview shader when its graph changed, and sends its
    /// parameter values (every frame: they're just uniforms).
    pub(super) fn update_preview_shader(&mut self) {
        // Editing a shading graph in the Material Designer previews it.
        if self.workspace == Workspace::Designer
            && let Some(doc) = self.graphs.editor.doc
            && self.graphs.doc(doc).is_some_and(|d| d.shading)
        {
            self.preview.graph = Some(doc);
        }
        let graph = self.preview.graph.and_then(|id| self.graphs.doc(id)).map(|d| &d.graph);
        let Some(graph) = graph else {
            if self.views.has_custom_shader() {
                let _ = self.views.set_custom_shader(&self.rs.device, None);
            }
            self.preview = Default::default();
            return;
        };
        match shading::compile(graph, &self.graphs.lib) {
            Ok(c) => {
                if c.code != self.preview.code || !self.views.has_custom_shader() {
                    match self.views.set_custom_shader(&self.rs.device, Some(&c.code)) {
                        Ok(()) => self.preview.error = None,
                        Err(e) => self.preview.error = Some(e),
                    }
                    self.preview.code = c.code.clone();
                }
                let values = shading::param_values(&c, graph, &self.graphs.lib);
                self.views.set_custom_params(&self.rs.queue, &values);
                self.preview.compiled = Some(c);
            }
            Err(e) => self.preview.error = Some(e),
        }
    }

    /// Starts a preview shader from preset `i` (offscreen UI tests); returns
    /// its graph id.
    #[cfg(test)]
    pub(crate) fn test_preview_preset(&mut self, i: usize) -> u64 {
        self.new_preview_shader(shading::presets().swap_remove(i));
        self.preview.graph.expect("set")
    }

    /// The preview shader's compile error, if any (offscreen UI tests).
    #[cfg(test)]
    pub(crate) fn test_preview_error(&self) -> Option<String> {
        self.preview.error.clone()
    }

    /// Adds a shading graph (a preset, or the starter) and makes it the
    /// preview shader.
    fn new_preview_shader(&mut self, graph: crate::graph::Graph) {
        let id = self.graphs.insert(&self.rs, graph, None);
        self.preview.graph = Some(id);
        self.status = "Added a preview shader. Edit its graph in the Material Designer; the 3D view uses it while you texture.".into();
    }

    /// The Preview panel's section for choosing and editing the shader.
    pub(super) fn preview_shader_section(&mut self, ui: &mut egui::Ui) {
        let shaders: Vec<(u64, String)> = self.graphs.docs.iter().filter(|d| d.shading).map(|d| (d.id, d.graph.name.clone())).collect();
        let current = self.preview.graph.and_then(|id| shaders.iter().find(|(s, _)| *s == id)).map_or("Default (PBR)".to_string(), |(_, n)| n.clone());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Shader").color(theme::TEXT_DIM));
            egui::ComboBox::from_id_salt("preview shader").selected_text(current).width(ui.available_width() - 66.0).show_ui(ui, |ui| {
                if ui.selectable_label(self.preview.graph.is_none(), "Default (PBR)").clicked() {
                    self.preview.graph = None;
                }
                for (id, name) in &shaders {
                    if ui.selectable_label(self.preview.graph == Some(*id), name).clicked() {
                        self.preview.graph = Some(*id);
                    }
                }
            });
            if ui.add_enabled(self.preview.graph.is_some(), egui::Button::new("Edit")).on_hover_text("Open its graph in the Material Designer").clicked()
                && let Some(id) = self.preview.graph
            {
                self.open_graph(id);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("New").small().color(theme::TEXT_DIM));
            for g in shading::presets() {
                if ui.small_button(&g.name).on_hover_text("Start a preview shader from this preset").clicked() {
                    self.new_preview_shader(g);
                }
            }
            if ui.small_button(theme::with_icon(icons::FILE_PLUS, "Blank")).on_hover_text("The PBR look as a graph, to change step by step").clicked() {
                self.new_preview_shader(shading::starter("Preview shader"));
            }
        });
        if let Some(e) = &self.preview.error {
            ui.label(egui::RichText::new(format!("The shader doesn't compile: {e}")).small().color(theme::ERROR));
        } else {
            ui.label(
                egui::RichText::new("Only changes how the 3D view looks while you work; exports are unaffected. Ray tracing uses the default look.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }
    }
}
