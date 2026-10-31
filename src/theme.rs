//! Dark, modern UI theme: neutral gray panels separated by a near-black
//! backdrop, square corners, selection as a blue outline on a darker
//! background, Segoe UI text
//! (semibold headings) and Phosphor icons mixed into labels.

use crate::icons;
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, vec2};

/// The backdrop between panels (the gaps make the panels stand apart).
pub const BG: Color32 = Color32::from_rgb(0x07, 0x07, 0x08);
/// Panel surfaces: neutral grays.
pub const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1c);
pub const PANEL_DARK: Color32 = Color32::from_rgb(0x12, 0x12, 0x13);
pub const HEADER: Color32 = Color32::from_rgb(0x22, 0x22, 0x24);
pub const WIDGET: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2d);
pub const WIDGET_HOVER: Color32 = Color32::from_rgb(0x36, 0x36, 0x3a);
/// Selection: a blue outline (never a fill) over [`SELECT_BG`].
pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0x8e, 0xff);
/// A faint blue tint (text selection, slider fill).
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x1c, 0x2c, 0x44);
/// The darker background of selected items.
pub const SELECT_BG: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0e);
pub const TEXT: Color32 = Color32::from_rgb(0xe4, 0xe4, 0xe6);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
pub const BORDER: Color32 = Color32::from_rgb(0x07, 0x07, 0x08);
pub const OUTLINE: Color32 = Color32::from_rgb(0x36, 0x36, 0x3c);

/// Corner radius of controls; cards use [`CARD_RADIUS`].
pub const RADIUS: u8 = 0;
pub const CARD_RADIUS: u8 = 0;

/// Font family of headings and card titles (Segoe UI Semibold when found).
pub fn heading_family() -> egui::FontFamily {
    egui::FontFamily::Name("heading".into())
}

/// A system font file, if this machine has it.
fn system_font(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var_os("WINDIR").map_or_else(|| std::path::PathBuf::from("C:/Windows"), std::path::PathBuf::from);
    std::fs::read(dir.join("Fonts").join(name)).ok()
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("phosphor".into(), egui::FontData::from_static(icons::FONT).into());
    let body = system_font("segoeui.ttf");
    let semibold = system_font("seguisb.ttf");
    if let Some(b) = body {
        fonts.font_data.insert("ui".into(), egui::FontData::from_owned(b).into());
        if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            list.insert(0, "ui".into());
        }
    }
    let mut heading: Vec<String> = Vec::new();
    if let Some(s) = semibold {
        fonts.font_data.insert("ui semibold".into(), egui::FontData::from_owned(s).into());
        heading.push("ui semibold".into());
    }
    // Headings fall back to the body fonts (and icons).
    heading.extend(fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(heading_family(), heading);
    // Symbols (box lines, shapes, check marks, dice) from Segoe UI Symbol.
    let symbols = system_font("seguisym.ttf").map(|b| fonts.font_data.insert("symbols".into(), egui::FontData::from_owned(b).into())).is_some();
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace, heading_family()] {
        if let Some(list) = fonts.families.get_mut(&family) {
            if !list.iter().any(|f| f == "phosphor") {
                list.push("phosphor".into());
            }
            if symbols {
                list.push("symbols".into());
            }
        }
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(style_mut);
}

fn style_mut(style: &mut egui::Style) {
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL_DARK;
    v.faint_bg_color = HEADER;
    v.code_bg_color = PANEL_DARK;
    v.override_text_color = Some(TEXT);
    // Selected things read as blue text or outline on a dark tint.
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0x8c, 0xbc, 0xff));
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_stroke = Stroke::new(1.0, OUTLINE);
    v.window_shadow = egui::Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(140) };
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.6 };
    v.striped = false;
    v.indent_has_left_vline = false;

    let r = CornerRadius::same(RADIUS);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, OUTLINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;
    for (state, fill) in [(&mut w.inactive, WIDGET), (&mut w.hovered, WIDGET_HOVER), (&mut w.active, SELECT_BG), (&mut w.open, SELECT_BG)] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, OUTLINE);
        state.fg_stroke = Stroke::new(1.0, TEXT);
        state.corner_radius = r;
        state.expansion = 0.0;
    }
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x55, 0x55, 0x5c));
    w.open.bg_stroke = Stroke::new(1.0, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    let s = &mut style.spacing;
    s.item_spacing = vec2(7.0, 6.0);
    s.button_padding = vec2(10.0, 4.0);
    s.interact_size.y = 24.0;
    s.slider_width = 150.0;
    s.slider_rail_height = 4.0;
    s.menu_margin = Margin::same(8);
    s.window_margin = Margin::same(14);
    s.combo_width = 120.0;
    for (text_style, size) in [
        (egui::TextStyle::Body, 13.0),
        (egui::TextStyle::Button, 13.0),
        (egui::TextStyle::Small, 11.0),
        (egui::TextStyle::Monospace, 12.5),
    ] {
        if let Some(f) = style.text_styles.get_mut(&text_style) {
            f.size = size;
        }
    }
    style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::new(17.0, heading_family()));
}

/// A floating card: rounded, softly outlined, with a gap around it showing
/// the backdrop.
pub fn card(fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(CARD_RADIUS))
        .stroke(Stroke::new(1.0, OUTLINE))
        .outer_margin(Margin::same(4))
        .inner_margin(Margin::same(8))
}

/// A thin bar on the backdrop (menu, status).
pub fn bar() -> egui::Frame {
    egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(10, 3))
}

/// A square icon button, accent-filled while `selected`.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, selected: bool, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let p = ui.painter();
    let (bg, fg) = if selected {
        (SELECT_BG, Color32::WHITE)
    } else if resp.hovered() {
        (WIDGET_HOVER, Color32::WHITE)
    } else {
        (Color32::TRANSPARENT, TEXT_DIM)
    };
    p.rect_filled(rect, CornerRadius::same(RADIUS), bg);
    if selected {
        p.rect_stroke(rect, CornerRadius::same(RADIUS), Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.56), fg);
    resp
}

/// A tab button: outlined in blue on a darker background while selected.
pub fn pill(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let fg = if selected { Color32::WHITE } else { TEXT_DIM };
    let button = egui::Button::new(egui::RichText::new(text).color(fg))
        .fill(if selected { SELECT_BG } else { Color32::TRANSPARENT })
        .stroke(if selected { Stroke::new(1.0, ACCENT) } else { Stroke::new(1.0, OUTLINE) })
        .corner_radius(CornerRadius::same(RADIUS))
        .min_size(vec2(0.0, 26.0));
    ui.add(button)
}

/// A toggle button: outlined in blue on a darker background while selected
/// (instead of egui's filled selection).
pub fn selectable<'a>(selected: bool, text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    let b = egui::Button::new(text);
    if selected {
        b.fill(SELECT_BG).stroke(Stroke::new(1.0, ACCENT))
    } else {
        b.fill(Color32::TRANSPARENT).stroke(Stroke::NONE)
    }
}

/// Icon followed by text, for buttons and tabs.
/// Ko-fi-like coral: the one saturated warm color in the UI, so the donate
/// button stands out against the greys and the blue accent.
pub const DONATE: Color32 = Color32::from_rgb(0xe8, 0x45, 0x4a);
const DONATE_HOVER: Color32 = Color32::from_rgb(0xff, 0x5e, 0x5b);

/// The donate button: coral fill, bold white text, a brighter fill on hover.
pub fn donate_button(ui: &mut egui::Ui, text: &str, min_size: egui::Vec2) -> egui::Response {
    // Hover state from last frame's response of this same widget.
    let id = ui.next_auto_id();
    let hot = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let label = egui::RichText::new(with_icon(crate::icons::HEART, text)).strong().color(Color32::WHITE);
    ui.add(
        egui::Button::new(label)
            .fill(if hot { DONATE_HOVER } else { DONATE })
            .stroke(egui::Stroke::NONE)
            .min_size(min_size),
    )
}

pub fn with_icon(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}
