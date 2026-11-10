//! Studio UI theme: warm dark greys, rounded corners, hairline borders and
//! soft shadows, a yellow accent, Space Grotesk text (medium headings),
//! JetBrains Mono for code and numbers, and Phosphor icons mixed into labels.

use crate::icons;
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, vec2};

/// The backdrop between panels (the gaps make the panels stand apart).
pub const BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
/// Panel surfaces.
pub const PANEL: Color32 = Color32::from_rgb(0x1c, 0x1d, 0x22);
/// Recessed areas inside panels: wells, lists, text fields.
pub const PANEL_DARK: Color32 = Color32::from_rgb(0x17, 0x18, 0x1c);
pub const HEADER: Color32 = Color32::from_rgb(0x22, 0x23, 0x29);
pub const WIDGET: Color32 = Color32::from_rgb(0x23, 0x24, 0x2a);
pub const WIDGET_HOVER: Color32 = Color32::from_rgb(0x32, 0x34, 0x3c);
/// The accent: focus, highlights and active handles.
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xce, 0x1b);
/// A dim accent fill (text selection, "on" buttons).
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x4a, 0x3f, 0x16);
/// The fill of selected items.
pub const SELECT_BG: Color32 = Color32::from_rgb(0x2b, 0x2c, 0x32);
/// The edge of selected items: a light rim, like brushed metal.
pub const SELECT_RIM: Color32 = Color32::from_rgb(0x4d, 0x4e, 0x57);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe4, 0xe1);
/// Text of hovered and selected things.
pub const TEXT_BRIGHT: Color32 = Color32::from_rgb(0xf4, 0xf3, 0xf0);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9c, 0x9c, 0xa6);
pub const BORDER: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
/// Hairline outlines of controls and cards.
pub const OUTLINE: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x33);
/// Outlines of hovered controls.
pub const OUTLINE_STRONG: Color32 = Color32::from_rgb(0x3e, 0x40, 0x49);

/// Corner radius of controls; cards and windows use [`CARD_RADIUS`], small
/// things (icon buttons, menu items) [`SMALL_RADIUS`].
pub const RADIUS: u8 = 5;
pub const CARD_RADIUS: u8 = 6;
pub const SMALL_RADIUS: u8 = 4;

/// The soft drop shadow under cards.
fn card_shadow() -> egui::Shadow {
    egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(70) }
}

/// Font family of headings and card titles (Space Grotesk Medium).
pub fn heading_family() -> egui::FontFamily {
    egui::FontFamily::Name("heading".into())
}

/// Font folders of this OS, searched for [`system_font`].
fn font_dirs() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:/Windows"), PathBuf::from);
        return vec![win.join("Fonts")];
    }
    let mut dirs = vec![PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    if cfg!(target_os = "macos") {
        dirs.extend([PathBuf::from("/System/Library/Fonts"), PathBuf::from("/Library/Fonts")]);
    }
    dirs
}

/// Every font file under the font folders, by file name (Linux keeps fonts
/// in per-family subfolders, so they are searched a few levels deep).
fn font_index() -> std::collections::HashMap<String, std::path::PathBuf> {
    fn walk(dir: &std::path::Path, depth: u32, out: &mut std::collections::HashMap<String, std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                if depth > 0 {
                    walk(&path, depth - 1, out);
                }
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                out.entry(name.to_ascii_lowercase()).or_insert(path);
            }
        }
    }
    let mut index = std::collections::HashMap::new();
    for dir in font_dirs() {
        walk(&dir, 4, &mut index);
    }
    index
}

/// The first of `names` (font file names, best first) this machine has.
fn system_font(index: &std::collections::HashMap<String, std::path::PathBuf>, names: &[&str]) -> Option<Vec<u8>> {
    names.iter().find_map(|n| index.get(&n.to_ascii_lowercase())).and_then(|p| std::fs::read(p).ok())
}

/// Interface fonts, bundled (SIL Open Font License, alongside in assets/fonts).
const UI_FONT: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk-Regular.ttf");
const UI_MEDIUM_FONT: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf");
const MONO_FONT: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("phosphor".into(), egui::FontData::from_static(icons::FONT).into());
    fonts.font_data.insert("ui".into(), egui::FontData::from_static(UI_FONT).into());
    fonts.font_data.insert("ui medium".into(), egui::FontData::from_static(UI_MEDIUM_FONT).into());
    fonts.font_data.insert("mono".into(), egui::FontData::from_static(MONO_FONT).into());
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        list.insert(0, "ui".into());
    }
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        list.insert(0, "mono".into());
    }
    let mut heading: Vec<String> = vec!["ui medium".into()];
    // Headings fall back to the body fonts (and icons).
    heading.extend(fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(heading_family(), heading);
    // Symbols (box lines, shapes, check marks, dice): Segoe UI Symbol, or
    // DejaVu Sans, which covers them on Linux, or Apple Symbols on macOS.
    let index = font_index();
    let symbols = system_font(&index, &["seguisym.ttf", "DejaVuSans.ttf", "NotoSansSymbols2-Regular.ttf", "Apple Symbols.ttf"]).map(|b| fonts.font_data.insert("symbols".into(), egui::FontData::from_owned(b).into())).is_some();
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
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0xff, 0xda, 0x55));
    v.hyperlink_color = ACCENT;
    v.window_corner_radius = CornerRadius::same(CARD_RADIUS);
    v.menu_corner_radius = CornerRadius::same(CARD_RADIUS);
    v.window_stroke = Stroke::new(1.0, OUTLINE_STRONG);
    v.window_shadow = egui::Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(140) };
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
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
    w.hovered.bg_stroke = Stroke::new(1.0, OUTLINE_STRONG);
    w.open.bg_stroke = Stroke::new(1.0, SELECT_RIM);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT_BRIGHT);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, TEXT_BRIGHT);

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
        .shadow(card_shadow())
        .outer_margin(Margin::same(4))
        .inner_margin(Margin::same(8))
}

/// A thin bar on the backdrop (menu, status).
pub fn bar() -> egui::Frame {
    egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(10, 3))
}

/// A square icon button, filled with a light rim while `selected`.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, selected: bool, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let p = ui.painter();
    let (bg, fg) = if selected {
        (SELECT_BG, TEXT_BRIGHT)
    } else if resp.hovered() {
        (Color32::from_white_alpha(18), TEXT_BRIGHT)
    } else {
        (Color32::TRANSPARENT, TEXT_DIM)
    };
    p.rect_filled(rect, CornerRadius::same(SMALL_RADIUS), bg);
    if selected {
        p.rect_stroke(rect, CornerRadius::same(SMALL_RADIUS), Stroke::new(1.0, SELECT_RIM), egui::StrokeKind::Inside);
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(size * 0.56), fg);
    resp
}

/// A tab button: filled with a light rim while selected.
pub fn pill(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let fg = if selected { TEXT_BRIGHT } else { TEXT_DIM };
    let button = egui::Button::new(egui::RichText::new(text).color(fg))
        .fill(if selected { SELECT_BG } else { Color32::TRANSPARENT })
        .stroke(if selected { Stroke::new(1.0, SELECT_RIM) } else { Stroke::new(1.0, OUTLINE) })
        .corner_radius(CornerRadius::same(RADIUS))
        .min_size(vec2(0.0, 26.0));
    ui.add(button)
}

/// A toggle button: filled with a light rim while selected (instead of
/// egui's accent selection).
pub fn selectable<'a>(selected: bool, text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    let b = egui::Button::new(text);
    if selected {
        b.fill(SELECT_BG).stroke(Stroke::new(1.0, SELECT_RIM))
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

/// One line of text cut to `width` with "…" when it doesn't fit. Check
/// `Galley::elided` to show the full text on hover.
pub fn elided(painter: &egui::Painter, text: &str, font: egui::FontId, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping { max_width: width.max(8.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    painter.layout_job(job)
}
