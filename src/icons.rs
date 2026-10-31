//! Phosphor icon glyphs (regular weight, MIT; see assets/fonts). The font is
//! a fallback of the proportional family, so icons mix into any label:
//! `format!("{} Paint", icons::PAINT_BRUSH)`.

pub const FONT: &[u8] = include_bytes!("../assets/fonts/Phosphor.ttf");

pub const PAINT_BRUSH: &str = "\u{E6F0}";
pub const ERASER: &str = "\u{E21E}";
pub const EYEDROPPER: &str = "\u{E568}";
pub const PAINT_BUCKET: &str = "\u{E392}";
pub const POLYGON: &str = "\u{E6D0}";
pub const CUBE: &str = "\u{E1DA}";
pub const STACK: &str = "\u{E466}";
pub const FOLDER: &str = "\u{E24A}";
pub const FOLDER_PLUS: &str = "\u{E258}";
pub const FOLDER_OPEN: &str = "\u{E256}";
pub const EYE: &str = "\u{E220}";
pub const EYE_SLASH: &str = "\u{E224}";
pub const TRASH: &str = "\u{E4A6}";
pub const COPY: &str = "\u{E1CA}";
pub const MASK: &str = "\u{E9F4}";
pub const GRID_FOUR: &str = "\u{E296}";
pub const SPHERE: &str = "\u{EE66}";
pub const IMAGES: &str = "\u{E836}";
pub const SPARKLE: &str = "\u{E6A2}";
pub const MAGIC_WAND: &str = "\u{E6B6}";
pub const FLOPPY_DISK: &str = "\u{E248}";
pub const EXPORT: &str = "\u{EAF0}";
pub const UNDO: &str = "\u{E038}";
pub const REDO: &str = "\u{E036}";
pub const SUN: &str = "\u{E472}";
pub const APERTURE: &str = "\u{E00A}";
pub const FIRE: &str = "\u{E242}";
pub const COLUMNS: &str = "\u{E546}";
pub const SQUARE: &str = "\u{E45E}";
pub const CUBE_TRANSPARENT: &str = "\u{EC7C}";
pub const INFO: &str = "\u{E2CE}";
pub const TRIANGLE: &str = "\u{E4B0}";
pub const GEAR: &str = "\u{E270}";
