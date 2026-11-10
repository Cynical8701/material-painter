//! The welcome window: a banner of materials made in the app, quick ways
//! to start, recent projects and links to the community.

use super::{Action, PainterApp, REPO_URL, theme};
use crate::icons;
use crate::mesh::Primitive;
use eframe::egui;
use egui::{Color32, Rect, Sense, pos2, vec2};

/// Community links. Empty ones show as "coming soon".
pub(crate) const DISCORD_URL: &str = "https://discord.gg/TqKD5xKp7Y";
pub(crate) const DONATE_URL: &str = "https://ko-fi.com/allaeddinerakik";

/// Banner rendered with the app itself (smart materials on the material ball).
const BANNER: &[u8] = include_bytes!("../../assets/ui/welcome.jpg");

const WIDTH: f32 = 780.0;

impl PainterApp {
    fn welcome_banner(&mut self, ctx: &egui::Context) -> egui::TextureId {
        self.welcome_tex
            .get_or_insert_with(|| {
                let img = image::load_from_memory(BANNER).expect("bundled banner decodes").into_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                ctx.load_texture("welcome banner", egui::ColorImage::from_rgba_unmultiplied(size, &img), Default::default())
            })
            .id()
    }

    pub(super) fn welcome_window(&mut self, ctx: &egui::Context) {
        if !self.show_welcome {
            return;
        }
        let banner = self.welcome_banner(ctx);
        let mut close = false;
        egui::Window::new("Welcome")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size([WIDTH, 0.0])
            .frame(egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                // Banner with the title over it, and a close button.
                let (r, _) = ui.allocate_exact_size(vec2(WIDTH, WIDTH * 480.0 / 1400.0), Sense::hover());
                let p = ui.painter();
                p.image(banner, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                p.text(
                    r.left_bottom() + vec2(22.0, -46.0),
                    egui::Align2::LEFT_BOTTOM,
                    "Material Painter",
                    egui::FontId::new(30.0, theme::heading_family()),
                    Color32::WHITE,
                );
                p.text(
                    r.left_bottom() + vec2(24.0, -22.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("Version {}  ·  Paint, bake and design PBR materials", env!("CARGO_PKG_VERSION")),
                    egui::FontId::proportional(13.0),
                    Color32::from_white_alpha(210),
                );
                let x = Rect::from_center_size(r.right_top() + vec2(-22.0, 22.0), vec2(28.0, 28.0));
                let xr = ui.interact(x, ui.make_persistent_id("welcome close"), Sense::click());
                ui.painter().rect_filled(x, 0.0, if xr.hovered() { Color32::from_black_alpha(200) } else { Color32::from_black_alpha(120) });
                ui.painter().text(x.center(), egui::Align2::CENTER_CENTER, icons::X, egui::FontId::proportional(15.0), Color32::WHITE);
                if xr.on_hover_text("Close").clicked() {
                    close = true;
                }

                egui::Frame::new().inner_margin(egui::Margin { left: 20, right: 20, top: 14, bottom: 14 }).show(ui, |ui| {
                    ui.columns(3, |cols| {
                        // Start.
                        let ui = &mut cols[0];
                        heading(ui, "Start");
                        if link_button(ui, icons::FOLDER_OPEN, "Open project…").clicked() {
                            self.pending.push(Action::OpenProject(None));
                            close = true;
                        }
                        if link_button(ui, icons::DOWNLOAD_SIMPLE, "Import a mesh…").clicked() {
                            self.pending.push(Action::ImportMesh);
                            close = true;
                        }
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("New project on").small().color(theme::TEXT_DIM));
                        for prim in [Primitive::MaterialBall, Primitive::RoundedCube, Primitive::Sphere, Primitive::Plane] {
                            if link_button(ui, icons::FILE_PLUS, prim.name()).clicked() {
                                self.pending.push(Action::NewProject(prim));
                                close = true;
                            }
                        }

                        // Recent projects.
                        let ui = &mut cols[1];
                        heading(ui, "Recent projects");
                        if self.recent.is_empty() {
                            ui.label(egui::RichText::new("Projects you save show up here.").color(theme::TEXT_DIM));
                        }
                        for path in self.recent.iter().take(7) {
                            let name = path.file_stem().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                            let folder = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
                            let resp = ui.add(row_button(ui, egui::RichText::new(&name).color(theme::TEXT_BRIGHT))).on_hover_text(path.display().to_string());
                            ui.add(egui::Label::new(egui::RichText::new(folder).small().color(theme::TEXT_DIM)).truncate());
                            if resp.clicked() {
                                self.pending.push(Action::OpenProject(Some(path.clone())));
                                close = true;
                            }
                        }

                        // Community.
                        let ui = &mut cols[2];
                        heading(ui, "Community");
                        url_button(ui, icons::DISCORD_LOGO, "Join the Discord", DISCORD_URL);
                        url_button(ui, icons::GITHUB_LOGO, "Source on GitHub", REPO_URL);
                        url_button(ui, icons::BOOK_OPEN, "Read the guide", &format!("{REPO_URL}#readme"));
                        url_button(ui, icons::BUG, "Report a problem", &format!("{REPO_URL}/issues/new"));
                        ui.add_space(8.0);
                        let resp = theme::donate_button(ui, "Support on Ko-fi", vec2(ui.available_width(), 38.0));
                        if resp.on_hover_text("Material Painter is free; donations keep it going").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(DONATE_URL));
                        }
                    });
                    ui.add_space(10.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.prefs.show_welcome, "Show this window at startup");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                });
            });
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_welcome = false;
        }
    }
}

fn heading(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).size(15.0).family(theme::heading_family()).color(theme::TEXT_BRIGHT));
    ui.add_space(4.0);
}

/// A full-width, left-aligned button whose frame appears on hover, so it
/// reads as a list entry until the pointer is over it.
fn row_button<'a>(ui: &egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new((text.into(), egui::Atom::grow()))
        .frame(true)
        .frame_when_inactive(false)
        .min_size(vec2(ui.available_width(), 24.0))
}

fn link_button(ui: &mut egui::Ui, icon: &str, text: &str) -> egui::Response {
    ui.add(row_button(ui, theme::with_icon(icon, text)))
}

/// A link; empty URLs show disabled ("coming soon").
fn url_button(ui: &mut egui::Ui, icon: &str, text: &str, url: &str) {
    let resp = ui.add_enabled(!url.is_empty(), row_button(ui, theme::with_icon(icon, text)));
    if url.is_empty() {
        resp.on_disabled_hover_text("Coming soon");
    } else if resp.on_hover_text(url).clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}
