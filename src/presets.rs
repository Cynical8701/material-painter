//! Shelf content: materials, smart materials and mask generators, plus small
//! CPU-rendered preview spheres for the shelf thumbnails.

use crate::engine::{Blend, FillParams, GenKind, Generator, MatValues};
use eframe::egui;
use glam::Vec3;

#[derive(Clone, Copy)]
pub struct MaterialPreset {
    pub name: &'static str,
    pub color: [f32; 3],
    pub rough: f32,
    pub metal: f32,
    pub color_var: f32,
    pub rough_var: f32,
    pub height_var: f32,
    pub var_scale: f32,
}

const fn mat(name: &'static str, color: [f32; 3], rough: f32, metal: f32) -> MaterialPreset {
    MaterialPreset { name, color, rough, metal, color_var: 0.0, rough_var: 0.0, height_var: 0.0, var_scale: 4.0 }
}

const fn rich(
    name: &'static str,
    color: [f32; 3],
    rough: f32,
    metal: f32,
    color_var: f32,
    rough_var: f32,
    height_var: f32,
    var_scale: f32,
) -> MaterialPreset {
    MaterialPreset { name, color, rough, metal, color_var, rough_var, height_var, var_scale }
}

pub const MATERIALS: &[MaterialPreset] = &[
    mat("Gold", [1.0, 0.77, 0.34], 0.22, 1.0),
    mat("Silver", [0.97, 0.96, 0.92], 0.18, 1.0),
    mat("Copper", [0.96, 0.64, 0.54], 0.28, 1.0),
    rich("Iron Raw", [0.56, 0.57, 0.58], 0.45, 1.0, 0.08, 0.15, 0.0, 6.0),
    mat("Aluminium", [0.91, 0.92, 0.92], 0.3, 1.0),
    mat("Chrome", [0.55, 0.56, 0.55], 0.05, 1.0),
    rich("Steel Brushed", [0.66, 0.66, 0.67], 0.32, 1.0, 0.04, 0.12, 0.0, 14.0),
    mat("Plastic Red", [0.72, 0.07, 0.05], 0.38, 0.0),
    mat("Plastic White", [0.86, 0.86, 0.84], 0.35, 0.0),
    mat("Plastic Black", [0.04, 0.04, 0.04], 0.45, 0.0),
    mat("Car Paint Blue", [0.05, 0.18, 0.55], 0.12, 0.0),
    mat("Rubber", [0.05, 0.05, 0.05], 0.88, 0.0),
    mat("Ceramic Teal", [0.18, 0.55, 0.55], 0.15, 0.0),
    rich("Leather Brown", [0.36, 0.2, 0.11], 0.6, 0.0, 0.18, 0.15, 0.25, 22.0),
    rich("Concrete", [0.56, 0.55, 0.52], 0.9, 0.0, 0.2, 0.1, 0.35, 9.0),
    rich("Rust", [0.44, 0.24, 0.13], 0.9, 0.0, 0.25, 0.15, 0.3, 7.0),
    rich("Fabric Olive", [0.32, 0.34, 0.2], 0.95, 0.0, 0.1, 0.0, 0.15, 40.0),
    rich("Stone Dark", [0.25, 0.24, 0.23], 0.82, 0.0, 0.3, 0.2, 0.5, 5.0),
    mat("Paint Red", [0.62, 0.08, 0.05], 0.45, 0.0),
    rich("Paint Olive", [0.27, 0.29, 0.17], 0.6, 0.0, 0.08, 0.1, 0.0, 6.0),
    rich("Wax", [0.93, 0.88, 0.76], 0.35, 0.0, 0.03, 0.05, 0.05, 5.0),
    rich("Oak", [0.34, 0.21, 0.12], 0.72, 0.0, 0.18, 0.15, 0.0, 3.0),
    rich("Brass", [0.62, 0.49, 0.28], 0.45, 1.0, 0.18, 0.4, 0.0, 5.0),
    rich("Iron Dark", [0.27, 0.27, 0.29], 0.58, 1.0, 0.15, 0.3, 0.0, 8.0),
    rich("Bone", [0.82, 0.77, 0.64], 0.55, 0.0, 0.06, 0.1, 0.0, 10.0),
    rich("Moss", [0.24, 0.32, 0.11], 0.95, 0.0, 0.25, 0.05, 0.3, 25.0),
    // Metals: F0 albedo (sRGB) from measured reflectance.
    mat("Titanium", [0.76, 0.73, 0.69], 0.3, 1.0),
    mat("Nickel", [0.83, 0.80, 0.74], 0.25, 1.0),
    rich("Bronze", [0.70, 0.50, 0.30], 0.35, 1.0, 0.1, 0.2, 0.0, 6.0),
    rich("Zinc Galvanized", [0.78, 0.80, 0.82], 0.42, 1.0, 0.1, 0.3, 0.0, 12.0),
    rich("Gunmetal", [0.33, 0.34, 0.36], 0.3, 1.0, 0.05, 0.1, 0.0, 8.0),
    mat("Rose Gold", [0.95, 0.70, 0.60], 0.22, 1.0),
    // Dielectrics.
    mat("Car Paint Red", [0.55, 0.03, 0.03], 0.1, 0.0),
    mat("Porcelain", [0.90, 0.89, 0.86], 0.08, 0.0),
    rich("Carbon Fiber", [0.06, 0.06, 0.065], 0.22, 0.0, 0.3, 0.1, 0.05, 60.0),
    mat("Obsidian", [0.03, 0.03, 0.035], 0.06, 0.0),
    rich("Marble White", [0.88, 0.87, 0.84], 0.12, 0.0, 0.08, 0.05, 0.0, 3.0),
    rich("Granite", [0.45, 0.43, 0.42], 0.55, 0.0, 0.45, 0.2, 0.1, 60.0),
    rich("Sandstone", [0.76, 0.62, 0.45], 0.9, 0.0, 0.15, 0.05, 0.3, 14.0),
    rich("Brick Red", [0.55, 0.24, 0.16], 0.88, 0.0, 0.2, 0.08, 0.3, 10.0),
    rich("Terracotta", [0.70, 0.38, 0.24], 0.8, 0.0, 0.1, 0.08, 0.15, 8.0),
    rich("Asphalt", [0.12, 0.12, 0.12], 0.92, 0.0, 0.35, 0.05, 0.35, 45.0),
    rich("Jade", [0.30, 0.58, 0.40], 0.2, 0.0, 0.2, 0.05, 0.0, 4.0),
    rich("Pine", [0.70, 0.52, 0.32], 0.7, 0.0, 0.12, 0.12, 0.0, 3.0),
    rich("Walnut", [0.24, 0.14, 0.08], 0.6, 0.0, 0.2, 0.15, 0.0, 3.0),
    rich("Ebony", [0.07, 0.05, 0.04], 0.35, 0.0, 0.15, 0.1, 0.0, 6.0),
    rich("Bark", [0.25, 0.19, 0.14], 0.92, 0.0, 0.25, 0.05, 0.5, 12.0),
    rich("Skin", [0.80, 0.58, 0.47], 0.5, 0.0, 0.06, 0.1, 0.05, 20.0),
    rich("Denim", [0.16, 0.24, 0.40], 0.9, 0.0, 0.15, 0.0, 0.1, 50.0),
    rich("Velvet Red", [0.40, 0.04, 0.07], 0.95, 0.0, 0.1, 0.0, 0.05, 30.0),
    rich("Paper", [0.88, 0.86, 0.80], 0.85, 0.0, 0.03, 0.05, 0.03, 40.0),
    rich("Cardboard", [0.60, 0.46, 0.30], 0.88, 0.0, 0.08, 0.05, 0.08, 30.0),
    rich("Snow", [0.92, 0.93, 0.95], 0.6, 0.0, 0.03, 0.15, 0.2, 10.0),
    mat("Ice", [0.70, 0.80, 0.86], 0.08, 0.0),
];

impl MaterialPreset {
    pub fn fill(&self) -> FillParams {
        FillParams {
            mat: MatValues {
                enabled: [true, true, true, self.height_var > 0.0],
                color: self.color,
                rough: self.rough,
                metal: self.metal,
                height: 0.0,
            },
            color_var: self.color_var,
            var_scale: self.var_scale,
            rough_var: self.rough_var,
            height_var: self.height_var,
            seed: 0.0,
            part_var: 0.0,
            ..FillParams::default()
        }
    }

    pub fn by_name(name: &str) -> &'static MaterialPreset {
        MATERIALS.iter().find(|m| m.name == name).expect("preset exists")
    }
}

/// A smart layer's mask: a generator preset with optional overrides, so each
/// layer can be tuned the way an artist would tune it in the mask panel.
#[derive(Clone, Copy)]
pub struct SmartMask {
    pub kind: GenKind,
    pub invert: bool,
    /// Input weights; when non-empty they replace the preset's weights.
    pub weights: &'static [(usize, f32)],
    pub level: Option<f32>,
    pub contrast: Option<f32>,
    pub scale: Option<f32>,
    pub noise: Option<f32>,
    pub edge_width: Option<f32>,
    pub seed: f32,
}

const fn mask(kind: GenKind) -> SmartMask {
    SmartMask { kind, invert: false, weights: &[], level: None, contrast: None, scale: None, noise: None, edge_width: None, seed: 0.0 }
}

/// Mask from explicit input weights, with level and contrast.
const fn mix(kind: GenKind, weights: &'static [(usize, f32)], level: f32, contrast: f32) -> SmartMask {
    SmartMask { weights, level: Some(level), contrast: Some(contrast), ..mask(kind) }
}

pub const COLOR_ONLY: [bool; 4] = [true, false, false, false];
pub const HEIGHT_ONLY: [bool; 4] = [false, false, false, true];
pub const ROUGH_ONLY: [bool; 4] = [false, true, false, false];

pub struct SmartLayer {
    pub name: &'static str,
    pub material: &'static str,
    /// Overrides of the material's values (color in sRGB).
    pub color: Option<[f32; 3]>,
    pub rough: Option<f32>,
    pub metal: Option<f32>,
    /// Fill height; enables the height channel.
    pub height: Option<f32>,
    pub color_var: Option<f32>,
    pub rough_var: Option<f32>,
    pub height_var: Option<f32>,
    pub var_scale: Option<f32>,
    pub blend: Blend,
    pub opacity: f32,
    pub mask: Option<SmartMask>,
    /// Channels the layer writes, when not the material's own.
    pub channels: Option<[bool; 4]>,
    /// Per-part brightness/roughness variation.
    pub part_var: f32,
    pub seed: f32,
    /// A scanned PBR set whose images replace the layer's values.
    pub scan: Option<Scan>,
}

/// A photo-scanned PBR texture set in `assets/pbr/<id>` (color.jpg,
/// rough.jpg, height.jpg, optional metal.png), projected tri-planar.
#[derive(Clone, Copy)]
pub struct Scan {
    pub id: &'static str,
    /// Repeats per world unit (meshes are about two units across).
    pub tiling: f32,
    /// Height strength of the scan's displacement map.
    pub height: f32,
}

impl Scan {
    pub const FILES: [(usize, &'static str); 4] = [(0, "color.jpg"), (1, "rough.jpg"), (2, "metal.png"), (3, "height.jpg")];
}

pub struct SmartMaterial {
    pub name: &'static str,
    pub layers: &'static [SmartLayer],
}

const fn layer(name: &'static str, material: &'static str) -> SmartLayer {
    SmartLayer {
        name,
        material,
        color: None,
        rough: None,
        metal: None,
        height: None,
        color_var: None,
        rough_var: None,
        height_var: None,
        var_scale: None,
        blend: Blend::Normal,
        opacity: 1.0,
        mask: None,
        channels: None,
        part_var: 0.0,
        seed: 0.0,
        scan: None,
    }
}

/// A base layer from a scanned set; `material` supplies the values used
/// where the scan has no map (metalness of non-metals) or is missing.
const fn scan(name: &'static str, material: &'static str, id: &'static str, tiling: f32, height: f32) -> SmartLayer {
    SmartLayer { scan: Some(Scan { id, tiling, height }), ..layer(name, material) }
}

impl SmartMaterial {
    /// The baked maps its masks read (a bit per `engine::MAPS` index).
    pub fn needs_maps(&self) -> u32 {
        self.layers.iter().filter_map(|l| l.generator()).fold(0, |acc, g| acc | g.needs_maps())
    }

    /// Built on a scanned texture set.
    pub fn is_scanned(&self) -> bool {
        self.layers.iter().any(|l| l.scan.is_some())
    }
}

use crate::engine::gin::*;

// Shared masks. Meshes are normalized to about two units across on import,
// so scales and edge widths here are relative to the whole object.
const GRIME: SmartMask = SmartMask {
    noise: Some(0.5),
    edge_width: Some(0.03),
    ..mix(GenKind::Dirt, &[(OCCLUSION, 2.5), (CAVITY, 1.5), (CREASE, 1.5)], 0.6, 0.2)
};
const DUST: SmartMask = SmartMask { noise: Some(0.8), scale: Some(4.0), ..mix(GenKind::Dust, &[(SKY, 0.8), (UP, 0.6), (OCCLUSION, 0.3)], 1.3, 0.3) };
const POLISHED_EDGES: SmartMask = SmartMask {
    noise: Some(0.6),
    edge_width: Some(0.006),
    ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.5)], 0.55, 0.1)
};
const SCRATCHED: SmartMask = SmartMask { scale: Some(2.0), noise: Some(0.0), ..mix(GenKind::Clouds, &[(SCRATCHES, 1.0)], 0.22, 0.12) };
/// Chips: edges plus blotches of noise, crisp.
const CHIPS: SmartMask = SmartMask {
    noise: Some(0.9),
    scale: Some(5.0),
    edge_width: Some(0.012),
    ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.6), (NOISE, 0.35)], 0.72, 0.04)
};
const DUST_LAYER: SmartLayer = SmartLayer {
    color: Some([0.52, 0.48, 0.42]),
    rough: Some(0.95),
    metal: Some(0.0),
    opacity: 0.4,
    mask: Some(DUST),
    ..layer("Dust", "Concrete")
};
const GRIME_LAYER: SmartLayer = SmartLayer {
    color: Some([0.10, 0.08, 0.055]),
    rough: Some(0.9),
    metal: Some(0.0),
    opacity: 0.6,
    mask: Some(GRIME),
    ..layer("Grime", "Concrete")
};
/// Fine surface detail every smart material gets on top: roughness overlaid
/// around 0.5 (varies what's below without flattening it) and a little
/// height added, at a scale far finer than the wear and dirt masks.
pub const MICRO_DETAIL: [SmartLayer; 2] = [
    SmartLayer {
        rough: Some(0.5),
        rough_var: Some(0.14),
        var_scale: Some(40.0),
        channels: Some(ROUGH_ONLY),
        blend: Blend::Overlay,
        ..layer("Micro Roughness", "Concrete")
    },
    SmartLayer {
        height: Some(0.0),
        height_var: Some(0.025),
        var_scale: Some(55.0),
        channels: Some(HEIGHT_ONLY),
        blend: Blend::Add,
        ..layer("Micro Height", "Concrete")
    },
];

/// Lighter grime for scanned sets, which already carry their own dirt.
const SCAN_GRIME: SmartLayer = SmartLayer { color: Some([0.12, 0.09, 0.06]), opacity: 0.4, ..GRIME_LAYER };

pub const SMART_MATERIALS: &[SmartMaterial] = &[
    SmartMaterial {
        name: "Oak Aged",
        layers: &[
            SmartLayer { part_var: 0.15, ..layer("Oak", "Oak") },
            SmartLayer {
                color: Some([0.17, 0.09, 0.045]),
                rough: Some(0.8),
                height: Some(-0.25),
                opacity: 0.65,
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.1), ..mix(GenKind::Clouds, &[(GRAIN, 1.0)], 0.45, 0.2) }),
                ..layer("Grain", "Oak")
            },
            SmartLayer {
                color: Some([0.52, 0.40, 0.28]),
                rough: Some(0.85),
                opacity: 0.35,
                mask: Some(SmartMask { scale: Some(1.5), noise: Some(0.0), ..mix(GenKind::Clouds, &[(NOISE, 0.6), (SKY, 0.6)], 0.6, 0.25) }),
                ..layer("Sun Bleach", "Oak")
            },
            SmartLayer {
                color: Some([0.55, 0.40, 0.26]),
                rough: Some(0.6),
                opacity: 0.6,
                mask: Some(SmartMask { edge_width: Some(0.008), noise: Some(0.7), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.5)], 0.6, 0.1) }),
                ..layer("Worn Edges", "Oak")
            },
            GRIME_LAYER,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Painted Wood Chipped",
        layers: &[
            SmartLayer { color: Some([0.40, 0.27, 0.16]), ..layer("Pine", "Oak") },
            SmartLayer {
                color: Some([0.22, 0.13, 0.07]),
                height: Some(-0.2),
                opacity: 0.6,
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.1), ..mix(GenKind::Clouds, &[(GRAIN, 1.0)], 0.45, 0.2) }),
                ..layer("Grain", "Oak")
            },
            SmartLayer {
                color: Some([0.20, 0.30, 0.32]),
                rough: Some(0.55),
                height: Some(0.08),
                color_var: Some(0.1),
                rough_var: Some(0.2),
                part_var: 0.1,
                mask: Some(SmartMask { invert: true, ..CHIPS }),
                ..layer("Paint", "Paint Olive")
            },
            SmartLayer {
                color: Some([0.72, 0.70, 0.62]),
                rough: Some(0.9),
                opacity: 0.4,
                channels: Some([true, true, false, false]),
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(SKY, 0.7), (NOISE, 0.6)], 0.75, 0.15) }),
                ..layer("Sun Fade", "Paint Olive")
            },
            GRIME_LAYER,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Brass Aged",
        layers: &[
            SmartLayer { part_var: 0.1, ..layer("Brass", "Brass") },
            SmartLayer {
                color: Some([0.20, 0.17, 0.10]),
                rough: Some(0.8),
                metal: Some(0.3),
                opacity: 0.85,
                mask: Some(SmartMask {
                    scale: Some(4.0),
                    noise: Some(0.6),
                    edge_width: Some(0.03),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (CAVITY, 1.5), (CREASE, 1.0), (NOISE, 1.2)], 0.5, 0.25)
                }),
                ..layer("Tarnish", "Brass")
            },
            SmartLayer {
                color: Some([0.30, 0.52, 0.42]),
                rough: Some(0.85),
                metal: Some(0.0),
                height: Some(0.05),
                opacity: 0.9,
                mask: Some(SmartMask { noise: Some(0.7), scale: Some(6.0), ..mix(GenKind::CavityGrime, &[(CAVITY, 2.0), (CREASE, 1.0)], 0.75, 0.12) }),
                ..layer("Verdigris", "Concrete")
            },
            SmartLayer {
                color: Some([0.82, 0.68, 0.44]),
                rough: Some(0.25),
                opacity: 0.6,
                mask: Some(SCRATCHED),
                ..layer("Scratches", "Brass")
            },
            SmartLayer { color: Some([0.88, 0.74, 0.48]), rough: Some(0.22), opacity: 0.85, mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Brass") },
        ],
    },
    SmartMaterial {
        name: "Gold Tarnished",
        layers: &[
            SmartLayer { color: Some([0.92, 0.72, 0.36]), rough_var: Some(0.25), var_scale: Some(5.0), ..layer("Gold", "Gold") },
            SmartLayer {
                color: Some([0.40, 0.28, 0.12]),
                rough: Some(0.6),
                opacity: 0.8,
                mask: Some(SmartMask {
                    noise: Some(0.5),
                    edge_width: Some(0.03),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (CAVITY, 1.5), (NOISE, 0.8)], 0.55, 0.2)
                }),
                ..layer("Tarnish", "Gold")
            },
            SmartLayer { color: Some([1.0, 0.82, 0.45]), rough: Some(0.15), opacity: 0.5, mask: Some(SCRATCHED), ..layer("Scratches", "Gold") },
            SmartLayer { rough: Some(0.1), mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Gold") },
            SmartLayer { opacity: 0.6, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Silver Tarnished",
        layers: &[
            layer("Silver", "Silver"),
            SmartLayer {
                rough: Some(0.32),
                opacity: 0.6,
                channels: Some(ROUGH_ONLY),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.5, 0.2) }),
                ..layer("Smudges", "Silver")
            },
            SmartLayer {
                color: Some([0.09, 0.08, 0.07]),
                rough: Some(0.55),
                metal: Some(0.6),
                mask: Some(SmartMask {
                    noise: Some(0.45),
                    edge_width: Some(0.03),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 2.0), (CAVITY, 2.0), (CREASE, 1.0)], 0.5, 0.2)
                }),
                ..layer("Black Tarnish", "Silver")
            },
            SmartLayer { rough: Some(0.12), opacity: 0.6, mask: Some(SCRATCHED), ..layer("Scratches", "Silver") },
            SmartLayer { rough: Some(0.08), mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Silver") },
        ],
    },
    SmartMaterial {
        name: "Iron Forged",
        layers: &[
            SmartLayer { part_var: 0.12, ..layer("Iron", "Iron Dark") },
            SmartLayer {
                height: Some(-0.12),
                blend: Blend::Add,
                channels: Some(HEIGHT_ONLY),
                mask: Some(SmartMask { invert: true, scale: Some(1.5), noise: Some(0.0), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.5, 0.35) }),
                ..layer("Hammer Dents", "Iron Dark")
            },
            SmartLayer {
                color: Some([0.17, 0.17, 0.19]),
                rough: Some(0.7),
                opacity: 0.8,
                mask: Some(SmartMask { scale: Some(2.5), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.55, 0.12) }),
                ..layer("Mill Scale", "Iron Dark")
            },
            SmartLayer {
                color: Some([0.33, 0.15, 0.06]),
                rough: Some(0.92),
                metal: Some(0.0),
                height: Some(0.12),
                opacity: 0.9,
                mask: Some(SmartMask {
                    scale: Some(8.0),
                    noise: Some(0.6),
                    edge_width: Some(0.015),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 2.0), (CAVITY, 2.0), (CREASE, 1.5), (NOISE, 0.6)], 0.85, 0.15)
                }),
                ..layer("Rust", "Rust")
            },
            SmartLayer { color: Some([0.58, 0.58, 0.60]), rough: Some(0.3), opacity: 0.85, mask: Some(POLISHED_EDGES), ..layer("Worn Edges", "Iron Raw") },
            GRIME_LAYER,
        ],
    },
    SmartMaterial {
        name: "Rusty Iron",
        layers: &[
            layer("Iron", "Iron Dark"),
            SmartLayer {
                color: Some([0.42, 0.20, 0.08]),
                height: Some(0.1),
                mask: Some(SmartMask {
                    scale: Some(2.5),
                    noise: Some(0.3),
                    ..mix(GenKind::Clouds, &[(NOISE, 1.0), (OCCLUSION, 0.6), (EDGE, -0.4)], 0.55, 0.08)
                }),
                ..layer("Rust", "Rust")
            },
            SmartLayer {
                color: Some([0.62, 0.32, 0.12]),
                rough: Some(0.95),
                opacity: 0.7,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(9.0), noise: Some(0.0), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.62, 0.1) }),
                ..layer("Bright Rust", "Rust")
            },
            SmartLayer {
                color: Some([0.16, 0.08, 0.04]),
                height: Some(-0.15),
                blend: Blend::Normal,
                opacity: 0.9,
                // Small irregular pits, clustered where the rust is heavy
                // rather than spread evenly like polka dots.
                mask: Some(SmartMask {
                    scale: Some(5.0),
                    noise: Some(0.5),
                    ..mix(GenKind::Clouds, &[(CELLS, -1.0), (NOISE, 0.7), (OCCLUSION, 0.4)], 0.3, 0.05)
                }),
                ..layer("Pitting", "Rust")
            },
            SmartLayer { color: Some([0.30, 0.15, 0.07]), opacity: 0.85, mask: Some(mask(GenKind::Drips)), ..layer("Rust Bleeding", "Rust") },
            SmartLayer { color: Some([0.17, 0.09, 0.05]), opacity: 0.9, mask: Some(mask(GenKind::CavityGrime)), ..layer("Deep Rust", "Rust") },
        ],
    },
    SmartMaterial {
        name: "Steel Scratched",
        layers: &[
            layer("Steel", "Steel Brushed"),
            SmartLayer {
                rough: Some(0.5),
                opacity: 0.7,
                channels: Some(ROUGH_ONLY),
                mask: Some(SmartMask { scale: Some(2.5), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.55, 0.2) }),
                ..layer("Smudges", "Steel Brushed")
            },
            SmartLayer { color: Some([0.80, 0.80, 0.82]), rough: Some(0.12), mask: Some(SCRATCHED), ..layer("Scratches", "Steel Brushed") },
            SmartLayer { color: Some([0.82, 0.82, 0.84]), rough: Some(0.15), mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Steel Brushed") },
            SmartLayer { opacity: 0.6, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Painted Steel Worn",
        layers: &[
            layer("Steel", "Steel Brushed"),
            SmartLayer {
                color: Some([0.36, 0.20, 0.10]),
                rough: Some(0.85),
                metal: Some(0.0),
                mask: Some(SmartMask { noise: Some(0.6), edge_width: Some(0.02), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.6), (NOISE, 0.35)], 0.6, 0.1) }),
                ..layer("Rust Under Chips", "Rust")
            },
            SmartLayer {
                color: Some([0.42, 0.07, 0.04]),
                height: Some(0.06),
                color_var: Some(0.1),
                rough_var: Some(0.15),
                part_var: 0.1,
                mask: Some(SmartMask { invert: true, ..CHIPS }),
                ..layer("Paint", "Paint Red")
            },
            SmartLayer { color: Some([0.60, 0.60, 0.62]), rough: Some(0.25), metal: Some(1.0), opacity: 0.8, mask: Some(SCRATCHED), ..layer("Scratches", "Steel Brushed") },
            GRIME_LAYER,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Military Olive",
        layers: &[
            SmartLayer { part_var: 0.25, height: Some(0.0), ..layer("Olive Paint", "Paint Olive") },
            SmartLayer {
                color: Some([0.48, 0.48, 0.47]),
                rough: Some(0.35),
                metal: Some(1.0),
                height: Some(-0.06),
                mask: Some(CHIPS),
                ..layer("Bare Metal Chips", "Steel Brushed")
            },
            SmartLayer { color: Some([0.45, 0.45, 0.44]), rough: Some(0.3), metal: Some(1.0), opacity: 0.7, mask: Some(SCRATCHED), ..layer("Scratches", "Steel Brushed") },
            SmartLayer {
                color: Some([0.26, 0.21, 0.14]),
                rough: Some(0.95),
                height: Some(0.08),
                opacity: 0.9,
                mask: Some(SmartMask { invert: true, noise: Some(0.6), scale: Some(3.0), ..mix(GenKind::PartGradient, &[(WORLD_HEIGHT, 1.0), (PART_GRADIENT, 0.3)], 0.3, 0.12) }),
                ..layer("Mud", "Concrete")
            },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Copper Weathered",
        layers: &[
            layer("Copper", "Copper"),
            SmartLayer {
                color: Some([0.42, 0.22, 0.14]),
                rough: Some(0.55),
                opacity: 0.85,
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(NOISE, 1.0), (OCCLUSION, 0.5)], 0.45, 0.2) }),
                ..layer("Oxidation", "Copper")
            },
            SmartLayer {
                color: Some([0.33, 0.50, 0.42]),
                rough: Some(0.85),
                metal: Some(0.0),
                height: Some(0.06),
                color_var: Some(0.2),
                mask: Some(SmartMask {
                    noise: Some(0.6),
                    edge_width: Some(0.03),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 2.0), (CAVITY, 1.5), (CREASE, 1.0), (NOISE, 0.5)], 0.85, 0.15)
                }),
                ..layer("Patina", "Concrete")
            },
            SmartLayer {
                color: Some([0.36, 0.52, 0.44]),
                rough: Some(0.8),
                metal: Some(0.0),
                opacity: 0.5,
                mask: Some(mask(GenKind::RainStreaks)),
                ..layer("Patina Streaks", "Concrete")
            },
            SmartLayer { color: Some([0.98, 0.68, 0.55]), rough: Some(0.2), opacity: 0.8, mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Copper") },
        ],
    },
    SmartMaterial {
        name: "Leather Worn",
        layers: &[
            layer("Leather", "Leather Brown"),
            SmartLayer {
                height: Some(-0.15),
                blend: Blend::Add,
                channels: Some(HEIGHT_ONLY),
                mask: Some(SmartMask { scale: Some(6.0), noise: Some(0.0), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.75, 0.15) }),
                ..layer("Pebble Grain", "Leather Brown")
            },
            SmartLayer {
                color: Some([0.12, 0.07, 0.04]),
                opacity: 0.8,
                channels: Some(COLOR_ONLY),
                mask: Some(mask(GenKind::Dirt)),
                ..layer("Darkening", "Leather Brown")
            },
            SmartLayer {
                color: Some([0.55, 0.38, 0.24]),
                rough: Some(0.45),
                mask: Some(SmartMask { edge_width: Some(0.015), noise: Some(0.7), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.6)], 0.55, 0.15) }),
                ..layer("Worn Edges", "Leather Brown")
            },
            SmartLayer { color: Some([0.50, 0.36, 0.24]), rough: Some(0.7), opacity: 0.5, mask: Some(SCRATCHED), ..layer("Scuffs", "Leather Brown") },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Rubber Dusty",
        layers: &[
            layer("Rubber", "Rubber"),
            SmartLayer { color: Some([0.16, 0.16, 0.16]), rough: Some(0.6), mask: Some(mask(GenKind::EdgeWear)), ..layer("Scuffs", "Rubber") },
            SmartLayer { color: Some([0.14, 0.14, 0.14]), rough: Some(0.7), opacity: 0.6, mask: Some(SCRATCHED), ..layer("Scratches", "Rubber") },
            SmartLayer { opacity: 0.6, ..DUST_LAYER },
        ],
    },
    SmartMaterial {
        name: "Concrete Weathered",
        layers: &[
            layer("Concrete", "Concrete"),
            SmartLayer {
                height: Some(-0.2),
                blend: Blend::Add,
                channels: Some(HEIGHT_ONLY),
                mask: Some(SmartMask { scale: Some(6.0), noise: Some(0.0), ..mix(GenKind::Clouds, &[(CELLS, -1.0)], -0.12, 0.05) }),
                ..layer("Pores", "Concrete")
            },
            SmartLayer {
                color: Some([0.3, 0.29, 0.26]),
                opacity: 0.85,
                channels: Some(COLOR_ONLY),
                mask: Some(mask(GenKind::RainStreaks)),
                ..layer("Rain Marks", "Concrete")
            },
            SmartLayer {
                color: Some([0.19, 0.24, 0.10]),
                rough: Some(0.95),
                height: Some(0.1),
                opacity: 0.9,
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.5), ..mix(GenKind::Dust, &[(SKY, 0.6), (OCCLUSION, 0.6), (NOISE, 0.9)], 1.25, 0.15) }),
                ..layer("Moss", "Moss")
            },
            SmartLayer { color: Some([0.12, 0.11, 0.1]), opacity: 0.8, channels: Some(COLOR_ONLY), mask: Some(GRIME), ..layer("Grime", "Concrete") },
        ],
    },
    SmartMaterial {
        name: "Stone Mossy",
        layers: &[
            SmartLayer { part_var: 0.2, ..layer("Stone", "Stone Dark") },
            SmartLayer {
                color: Some([0.10, 0.10, 0.10]),
                height: Some(-0.35),
                mask: Some(SmartMask { scale: Some(1.2), noise: Some(0.25), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.88, 0.05) }),
                ..layer("Cracks", "Stone Dark")
            },
            SmartLayer {
                color: Some([0.45, 0.44, 0.40]),
                rough: Some(0.75),
                opacity: 0.6,
                mask: Some(SmartMask { edge_width: Some(0.02), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.6)], 0.5, 0.2) }),
                ..layer("Weathered Edges", "Stone Dark")
            },
            SmartLayer {
                color: Some([0.18, 0.25, 0.09]),
                height: Some(0.15),
                height_var: Some(0.3),
                var_scale: Some(30.0),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.6), ..mix(GenKind::Dust, &[(SKY, 0.6), (OCCLUSION, 0.6), (NOISE, 0.9)], 1.2, 0.15) }),
                ..layer("Moss", "Moss")
            },
            GRIME_LAYER,
        ],
    },
    SmartMaterial {
        name: "Bone Aged",
        layers: &[
            SmartLayer { color_var: Some(0.08), height_var: Some(0.1), var_scale: Some(12.0), ..layer("Bone", "Bone") },
            SmartLayer {
                color: Some([0.62, 0.50, 0.30]),
                opacity: 0.8,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { noise: Some(0.6), ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (CAVITY, 1.0), (NOISE, 0.6)], 0.45, 0.25) }),
                ..layer("Yellowing", "Bone")
            },
            SmartLayer {
                color: Some([0.30, 0.24, 0.16]),
                height: Some(-0.15),
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.2), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.92, 0.03) }),
                ..layer("Hairline Cracks", "Bone")
            },
            SmartLayer { opacity: 0.85, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Sci-Fi Panels",
        layers: &[
            SmartLayer { part_var: 0.15, ..layer("Panels", "Plastic White") },
            SmartLayer {
                color: Some([0.08, 0.08, 0.09]),
                rough: Some(0.7),
                mask: Some(mask(GenKind::CreaseGrime)),
                ..layer("Panel Gaps", "Plastic Black")
            },
            SmartLayer { color: Some([0.55, 0.56, 0.58]), rough: Some(0.3), metal: Some(1.0), opacity: 0.8, mask: Some(CHIPS), ..layer("Chipped Edges", "Aluminium") },
            SmartLayer { color: Some([0.70, 0.70, 0.70]), rough: Some(0.25), opacity: 0.5, mask: Some(SCRATCHED), ..layer("Scratches", "Plastic White") },
            SmartLayer { opacity: 0.5, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Candle Wax",
        layers: &[
            layer("Wax", "Wax"),
            SmartLayer {
                color: Some([1.0, 0.86, 0.6]),
                opacity: 0.9,
                channels: Some(COLOR_ONLY),
                mask: Some(mask(GenKind::ThinAreas)),
                ..layer("Light Through Thin Wax", "Wax")
            },
            SmartLayer { color: Some([0.45, 0.36, 0.25]), opacity: 0.7, channels: Some(COLOR_ONLY), mask: Some(mask(GenKind::CavityGrime)), ..layer("Soot", "Wax") },
        ],
    },
    SmartMaterial {
        name: "Galvanized Metal",
        layers: &[
            SmartLayer { part_var: 0.1, ..layer("Zinc", "Zinc Galvanized") },
            SmartLayer {
                color: Some([0.62, 0.64, 0.66]),
                rough: Some(0.3),
                opacity: 0.6,
                mask: Some(SmartMask { scale: Some(1.5), noise: Some(0.2), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.6, 0.06) }),
                ..layer("Spangle", "Zinc Galvanized")
            },
            SmartLayer {
                color: Some([0.80, 0.80, 0.78]),
                rough: Some(0.95),
                metal: Some(0.0),
                height: Some(0.04),
                opacity: 0.85,
                mask: Some(SmartMask {
                    scale: Some(5.0),
                    noise: Some(0.7),
                    edge_width: Some(0.03),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (CAVITY, 1.5), (CREASE, 1.0), (NOISE, 0.6)], 0.8, 0.12)
                }),
                ..layer("White Rust", "Concrete")
            },
            SmartLayer {
                color: Some([0.30, 0.29, 0.27]),
                rough: Some(0.7),
                opacity: 0.6,
                channels: Some([true, true, false, false]),
                mask: Some(mask(GenKind::RainStreaks)),
                ..layer("Water Streaks", "Zinc Galvanized")
            },
            SmartLayer { color: Some([0.88, 0.89, 0.90]), rough: Some(0.2), opacity: 0.6, mask: Some(SCRATCHED), ..layer("Scratches", "Zinc Galvanized") },
            SmartLayer { opacity: 0.6, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Bronze Patina",
        layers: &[
            SmartLayer { part_var: 0.1, ..layer("Bronze", "Bronze") },
            SmartLayer {
                color: Some([0.28, 0.17, 0.09]),
                rough: Some(0.55),
                metal: Some(0.6),
                opacity: 0.9,
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(NOISE, 1.0), (OCCLUSION, 0.6), (CONVEX, -0.5)], 0.35, 0.2) }),
                ..layer("Brown Oxide", "Bronze")
            },
            SmartLayer {
                color: Some([0.32, 0.62, 0.52]),
                rough: Some(0.9),
                metal: Some(0.0),
                height: Some(0.1),
                color_var: Some(0.2),
                var_scale: Some(20.0),
                mask: Some(SmartMask {
                    scale: Some(6.0),
                    noise: Some(0.7),
                    edge_width: Some(0.04),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 2.0), (CAVITY, 2.0), (CREASE, 1.5), (NOISE, 0.5)], 0.8, 0.12)
                }),
                ..layer("Verdigris", "Concrete")
            },
            SmartLayer {
                color: Some([0.38, 0.62, 0.54]),
                rough: Some(0.85),
                metal: Some(0.0),
                opacity: 0.55,
                mask: Some(SmartMask { noise: Some(0.25), ..mix(GenKind::RainStreaks, &[(SKY, 0.4), (STREAKS, 1.0), (OVERHANG, -0.4), (OCCLUSION, 0.3)], 0.95, 0.12) }),
                ..layer("Verdigris Runs", "Concrete")
            },
            SmartLayer {
                color: Some([0.86, 0.66, 0.40]),
                rough: Some(0.18),
                mask: Some(SmartMask { edge_width: Some(0.02), noise: Some(0.4), ..mix(GenKind::EdgeWear, &[(EDGE, 0.8), (CONVEX, 1.2)], 0.5, 0.15) }),
                ..layer("Hand-Rubbed Highlights", "Bronze")
            },
        ],
    },
    SmartMaterial {
        name: "Car Paint Chipped",
        layers: &[
            layer("Steel", "Steel Brushed"),
            SmartLayer {
                color: Some([0.52, 0.52, 0.50]),
                rough: Some(0.7),
                metal: Some(0.0),
                height: Some(0.04),
                mask: Some(SmartMask { invert: true, level: Some(0.9), ..CHIPS }),
                ..layer("Primer", "Plastic White")
            },
            SmartLayer { height: Some(0.08), part_var: 0.05, mask: Some(SmartMask { invert: true, ..CHIPS }), ..layer("Paint", "Car Paint Red") },
            SmartLayer {
                rough: Some(0.3),
                opacity: 0.8,
                channels: Some(ROUGH_ONLY),
                mask: Some(SmartMask { scale: Some(3.0), ..SCRATCHED }),
                ..layer("Clear-coat Swirls", "Car Paint Red")
            },
            SmartLayer {
                color: Some([0.20, 0.10, 0.06]),
                rough: Some(0.9),
                opacity: 0.6,
                mask: Some(SmartMask { invert: true, noise: Some(0.6), scale: Some(3.0), ..mix(GenKind::PartGradient, &[(WORLD_HEIGHT, 1.0), (PART_GRADIENT, 0.3)], 0.25, 0.12) }),
                ..layer("Road Grime", "Concrete")
            },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Plastic Worn",
        layers: &[
            SmartLayer { color: Some([0.85, 0.62, 0.08]), part_var: 0.08, ..layer("Plastic", "Plastic Red") },
            SmartLayer {
                color: Some([0.88, 0.78, 0.50]),
                opacity: 0.5,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(SKY, 0.8), (NOISE, 0.5)], 0.7, 0.2) }),
                ..layer("Sun Fade", "Plastic Red")
            },
            SmartLayer { rough: Some(0.15), opacity: 0.9, channels: Some(ROUGH_ONLY), mask: Some(POLISHED_EDGES), ..layer("Handled Shine", "Plastic Red") },
            SmartLayer { color: Some([0.95, 0.88, 0.70]), rough: Some(0.6), opacity: 0.7, mask: Some(SCRATCHED), ..layer("Stress Whitening", "Plastic Red") },
            SmartLayer { opacity: 0.65, ..GRIME_LAYER },
            SmartLayer { opacity: 0.6, ..DUST_LAYER },
        ],
    },
    SmartMaterial {
        name: "Gunmetal Blued",
        layers: &[
            SmartLayer { color: Some([0.16, 0.19, 0.26]), rough: Some(0.28), ..layer("Bluing", "Gunmetal") },
            SmartLayer {
                color: Some([0.30, 0.22, 0.24]),
                opacity: 0.45,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(NOISE, 0.8), (CONVEX, 0.6)], 0.55, 0.2) }),
                ..layer("Plum Fade", "Gunmetal")
            },
            SmartLayer {
                rough: Some(0.5),
                opacity: 0.6,
                channels: Some(ROUGH_ONLY),
                mask: Some(SmartMask { scale: Some(3.5), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.58, 0.12) }),
                ..layer("Fingerprints", "Gunmetal")
            },
            SmartLayer {
                color: Some([0.72, 0.72, 0.74]),
                rough: Some(0.22),
                mask: Some(SmartMask { edge_width: Some(0.012), ..POLISHED_EDGES }),
                ..layer("Holster Wear", "Steel Brushed")
            },
            SmartLayer { color: Some([0.62, 0.63, 0.66]), rough: Some(0.2), opacity: 0.7, mask: Some(SCRATCHED), ..layer("Scratches", "Steel Brushed") },
            SmartLayer { color: Some([0.04, 0.035, 0.03]), rough: Some(0.45), opacity: 0.8, mask: Some(GRIME), ..layer("Gun Oil Grime", "Gunmetal") },
        ],
    },
    SmartMaterial {
        name: "Marble Polished",
        layers: &[
            SmartLayer { part_var: 0.06, ..layer("Marble", "Marble White") },
            SmartLayer {
                color: Some([0.70, 0.70, 0.68]),
                opacity: 0.6,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(1.2), noise: Some(0.6), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.55, 0.25) }),
                ..layer("Clouding", "Marble White")
            },
            SmartLayer {
                color: Some([0.38, 0.40, 0.38]),
                opacity: 0.85,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(0.7), noise: Some(0.5), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.9, 0.04) }),
                ..layer("Veins", "Marble White")
            },
            SmartLayer {
                color: Some([0.55, 0.50, 0.42]),
                opacity: 0.5,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(1.6), noise: Some(0.5), seed: 3.0, ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.92, 0.03) }),
                ..layer("Gold Veins", "Marble White")
            },
            SmartLayer {
                rough: Some(0.45),
                opacity: 0.7,
                channels: Some(ROUGH_ONLY),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.5), ..mix(GenKind::Clouds, &[(SKY, 0.6), (NOISE, 0.7)], 0.75, 0.15) }),
                ..layer("Water Etching", "Marble White")
            },
            SmartLayer { color: Some([0.30, 0.27, 0.22]), opacity: 0.6, channels: Some(COLOR_ONLY), mask: Some(GRIME), ..layer("Grime", "Marble White") },
        ],
    },
    SmartMaterial {
        name: "Brick Weathered",
        layers: &[
            SmartLayer { part_var: 0.3, ..layer("Brick", "Brick Red") },
            SmartLayer {
                color: Some([0.30, 0.13, 0.08]),
                opacity: 0.6,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(2.5), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.6, 0.15) }),
                ..layer("Kiln Burn", "Brick Red")
            },
            SmartLayer {
                height: Some(-0.2),
                blend: Blend::Add,
                channels: Some(HEIGHT_ONLY),
                mask: Some(SmartMask { scale: Some(5.0), noise: Some(0.0), ..mix(GenKind::Clouds, &[(CELLS, -1.0)], -0.15, 0.05) }),
                ..layer("Pores", "Brick Red")
            },
            SmartLayer {
                color: Some([0.70, 0.40, 0.28]),
                height: Some(-0.15),
                opacity: 0.85,
                mask: Some(SmartMask { edge_width: Some(0.02), ..CHIPS }),
                ..layer("Chipped Edges", "Brick Red")
            },
            SmartLayer {
                color: Some([0.86, 0.84, 0.78]),
                rough: Some(0.95),
                opacity: 0.7,
                mask: Some(SmartMask {
                    invert: true,
                    noise: Some(0.7),
                    scale: Some(4.0),
                    ..mix(GenKind::PartGradient, &[(WORLD_HEIGHT, 1.0), (OCCLUSION, -0.3)], 0.3, 0.12)
                }),
                ..layer("Efflorescence", "Concrete")
            },
            SmartLayer {
                color: Some([0.18, 0.12, 0.09]),
                opacity: 0.6,
                channels: Some(COLOR_ONLY),
                mask: Some(mask(GenKind::RainStreaks)),
                ..layer("Rain Streaks", "Brick Red")
            },
            GRIME_LAYER,
        ],
    },
    SmartMaterial {
        name: "Ceramic Glazed Crazed",
        layers: &[
            SmartLayer { color: Some([0.86, 0.82, 0.70]), ..layer("Glaze", "Porcelain") },
            SmartLayer {
                color: Some([0.68, 0.60, 0.44]),
                opacity: 0.7,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { noise: Some(0.3), ..mix(GenKind::Dirt, &[(OCCLUSION, 1.2), (CAVITY, 1.2), (CONCAVE, 0.6)], 0.4, 0.25) }),
                ..layer("Glaze Pooling", "Porcelain")
            },
            SmartLayer {
                color: Some([0.70, 0.45, 0.30]),
                rough: Some(0.6),
                opacity: 0.6,
                mask: Some(SmartMask { edge_width: Some(0.008), noise: Some(0.5), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.6)], 0.6, 0.15) }),
                ..layer("Thin Glaze on Edges", "Terracotta")
            },
            SmartLayer {
                color: Some([0.30, 0.25, 0.18]),
                height: Some(-0.08),
                opacity: 0.8,
                channels: Some([true, false, false, true]),
                mask: Some(SmartMask { scale: Some(4.0), noise: Some(0.1), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.93, 0.025) }),
                ..layer("Crazing", "Porcelain")
            },
            SmartLayer { opacity: 0.5, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Chrome Pitted",
        layers: &[
            layer("Chrome", "Chrome"),
            SmartLayer {
                rough: Some(0.3),
                opacity: 0.6,
                channels: Some(ROUGH_ONLY),
                mask: Some(SmartMask { scale: Some(2.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(NOISE, 1.0), (OCCLUSION, 0.4)], 0.6, 0.15) }),
                ..layer("Haze", "Chrome")
            },
            SmartLayer {
                color: Some([0.70, 0.68, 0.62]),
                rough: Some(0.45),
                opacity: 0.8,
                mask: Some(SmartMask {
                    noise: Some(0.6),
                    edge_width: Some(0.03),
                    ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (CAVITY, 1.5), (CREASE, 1.0), (NOISE, 0.5)], 0.7, 0.12)
                }),
                ..layer("Flaked to Nickel", "Nickel")
            },
            SmartLayer {
                color: Some([0.38, 0.18, 0.08]),
                rough: Some(0.9),
                metal: Some(0.0),
                height: Some(-0.1),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.35), ..mix(GenKind::Clouds, &[(CELLS, -1.0), (OCCLUSION, 0.4), (NOISE, 0.3)], 0.0, 0.04) }),
                ..layer("Rust Pits", "Rust")
            },
            SmartLayer { color: Some([0.34, 0.17, 0.08]), opacity: 0.7, mask: Some(mask(GenKind::Drips)), ..layer("Rust Bleeding", "Rust") },
            SmartLayer { opacity: 0.6, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Canvas Dirty",
        layers: &[
            SmartLayer { color: Some([0.62, 0.56, 0.42]), part_var: 0.1, ..layer("Canvas", "Fabric Olive") },
            SmartLayer {
                color: Some([0.40, 0.34, 0.24]),
                opacity: 0.5,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(2.5), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.62, 0.1) }),
                ..layer("Stains", "Fabric Olive")
            },
            SmartLayer {
                color: Some([0.78, 0.74, 0.62]),
                opacity: 0.5,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { edge_width: Some(0.015), noise: Some(0.7), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.8)], 0.5, 0.15) }),
                ..layer("Frayed Edges", "Fabric Olive")
            },
            SmartLayer {
                color: Some([0.24, 0.19, 0.13]),
                height: Some(0.05),
                opacity: 0.85,
                mask: Some(SmartMask { invert: true, noise: Some(0.6), scale: Some(3.0), ..mix(GenKind::PartGradient, &[(WORLD_HEIGHT, 1.0), (PART_GRADIENT, 0.3)], 0.3, 0.12) }),
                ..layer("Mud", "Concrete")
            },
            GRIME_LAYER,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Bark Mossy",
        layers: &[
            SmartLayer { part_var: 0.1, ..layer("Bark", "Bark") },
            SmartLayer {
                color: Some([0.09, 0.07, 0.05]),
                height: Some(-0.45),
                mask: Some(SmartMask { scale: Some(1.2), noise: Some(0.3), ..mix(GenKind::Clouds, &[(GRAIN, 1.0), (NOISE, 0.4)], 0.6, 0.12) }),
                ..layer("Furrows", "Bark")
            },
            SmartLayer {
                color: Some([0.42, 0.38, 0.32]),
                opacity: 0.6,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { edge_width: Some(0.02), noise: Some(0.5), ..mix(GenKind::EdgeWear, &[(EDGE, 0.6), (CONVEX, 1.0)], 0.45, 0.15) }),
                ..layer("Weathered Ridges", "Bark")
            },
            SmartLayer {
                color: Some([0.62, 0.66, 0.52]),
                height: Some(0.05),
                opacity: 0.8,
                mask: Some(SmartMask { scale: Some(4.0), noise: Some(0.4), ..mix(GenKind::Clouds, &[(CELLS, -1.0), (NOISE, 0.5)], 0.05, 0.05) }),
                ..layer("Lichen", "Moss")
            },
            SmartLayer {
                color: Some([0.18, 0.27, 0.08]),
                height: Some(0.15),
                height_var: Some(0.3),
                var_scale: Some(30.0),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.6), ..mix(GenKind::Dust, &[(SKY, 0.5), (OCCLUSION, 0.6), (OVERHANG, -0.4), (NOISE, 0.9)], 1.0, 0.15) }),
                ..layer("Moss", "Moss")
            },
            GRIME_LAYER,
        ],
    },
    SmartMaterial {
        name: "Jade Carved",
        layers: &[
            layer("Jade", "Jade"),
            SmartLayer {
                color: Some([0.70, 0.80, 0.68]),
                opacity: 0.55,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(1.5), noise: Some(0.6), ..mix(GenKind::Clouds, &[(NOISE, 1.0)], 0.6, 0.15) }),
                ..layer("Pale Mottling", "Jade")
            },
            SmartLayer {
                color: Some([0.10, 0.30, 0.18]),
                opacity: 0.6,
                channels: Some(COLOR_ONLY),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.5), seed: 2.0, ..mix(GenKind::Clouds, &[(NOISE, 1.0), (CELLS, 0.4)], 0.75, 0.12) }),
                ..layer("Deep Inclusions", "Jade")
            },
            SmartLayer {
                color: Some([0.55, 0.85, 0.60]),
                opacity: 0.7,
                channels: Some(COLOR_ONLY),
                mask: Some(mask(GenKind::ThinAreas)),
                ..layer("Translucent Thin Areas", "Jade")
            },
            SmartLayer { rough: Some(0.08), channels: Some(ROUGH_ONLY), mask: Some(POLISHED_EDGES), ..layer("Polished Highs", "Jade") },
            SmartLayer { color: Some([0.20, 0.17, 0.10]), rough: Some(0.7), opacity: 0.7, mask: Some(mask(GenKind::CavityGrime)), ..layer("Carving Dirt", "Concrete") },
        ],
    },
    SmartMaterial {
        name: "Walnut Varnished",
        layers: &[
            SmartLayer { part_var: 0.12, ..layer("Walnut", "Walnut") },
            SmartLayer {
                color: Some([0.10, 0.05, 0.025]),
                height: Some(-0.15),
                opacity: 0.7,
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.1), ..mix(GenKind::Clouds, &[(GRAIN, 1.0)], 0.45, 0.2) }),
                ..layer("Grain", "Walnut")
            },
            SmartLayer { rough: Some(0.12), channels: Some(ROUGH_ONLY), ..layer("Varnish", "Walnut") },
            SmartLayer {
                color: Some([0.40, 0.26, 0.15]),
                rough: Some(0.6),
                opacity: 0.8,
                mask: Some(SmartMask { edge_width: Some(0.01), noise: Some(0.7), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.5)], 0.6, 0.1) }),
                ..layer("Varnish Worn Through", "Walnut")
            },
            SmartLayer { rough: Some(0.35), opacity: 0.6, channels: Some(ROUGH_ONLY), mask: Some(SCRATCHED), ..layer("Fine Scratches", "Walnut") },
            SmartLayer { opacity: 0.7, ..GRIME_LAYER },
            SmartLayer { opacity: 0.3, ..DUST_LAYER },
        ],
    },
    SmartMaterial {
        name: "Sci-Fi Hull Dark",
        layers: &[
            SmartLayer { color: Some([0.22, 0.23, 0.25]), rough: Some(0.45), metal: Some(0.0), part_var: 0.3, ..layer("Hull Paint", "Gunmetal") },
            SmartLayer {
                color: Some([0.85, 0.42, 0.08]),
                rough: Some(0.4),
                metal: Some(0.0),
                mask: Some(SmartMask { level: Some(0.78), ..mask(GenKind::PartRandom) }),
                ..layer("Accent Parts", "Paint Red")
            },
            SmartLayer { color: Some([0.03, 0.03, 0.035]), rough: Some(0.7), mask: Some(mask(GenKind::CreaseGrime)), ..layer("Panel Gaps", "Plastic Black") },
            SmartLayer { color: Some([0.70, 0.71, 0.72]), rough: Some(0.3), metal: Some(1.0), height: Some(-0.04), mask: Some(CHIPS), ..layer("Chipped to Titanium", "Titanium") },
            SmartLayer {
                color: Some([0.05, 0.045, 0.04]),
                rough: Some(0.8),
                metal: Some(0.0),
                opacity: 0.85,
                mask: Some(SmartMask {
                    invert: true,
                    noise: Some(0.8),
                    scale: Some(2.5),
                    ..mix(GenKind::PositionGradient, &[(WORLD_HEIGHT, 1.0), (OVERHANG, -0.4), (NOISE, 0.3)], 0.35, 0.15)
                }),
                ..layer("Engine Scorch", "Plastic Black")
            },
            SmartLayer { color: Some([0.12, 0.11, 0.10]), opacity: 0.6, channels: Some(COLOR_ONLY), mask: Some(mask(GenKind::Drips)), ..layer("Leaks", "Plastic Black") },
            SmartLayer { opacity: 0.5, ..GRIME_LAYER },
        ],
    },
    SmartMaterial {
        name: "Titanium Heat Tinted",
        layers: &[
            SmartLayer { rough_var: Some(0.15), var_scale: Some(8.0), ..layer("Titanium", "Titanium") },
            SmartLayer {
                color: Some([0.86, 0.68, 0.36]),
                rough: Some(0.25),
                mask: Some(SmartMask { scale: Some(1.2), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0), (WORLD_HEIGHT, -0.4)], 0.30, 0.06) }),
                ..layer("Straw Tint", "Titanium")
            },
            SmartLayer {
                color: Some([0.50, 0.30, 0.55]),
                rough: Some(0.22),
                mask: Some(SmartMask { scale: Some(1.2), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0), (WORLD_HEIGHT, -0.4)], 0.38, 0.05) }),
                ..layer("Purple Tint", "Titanium")
            },
            SmartLayer {
                color: Some([0.25, 0.42, 0.82]),
                rough: Some(0.2),
                mask: Some(SmartMask { scale: Some(1.2), noise: Some(0.3), ..mix(GenKind::Clouds, &[(NOISE, 1.0), (WORLD_HEIGHT, -0.4)], 0.46, 0.05) }),
                ..layer("Blue Tint", "Titanium")
            },
            SmartLayer { color: Some([0.82, 0.80, 0.76]), rough: Some(0.18), opacity: 0.7, mask: Some(SCRATCHED), ..layer("Scratches", "Titanium") },
            SmartLayer { color: Some([0.84, 0.82, 0.78]), rough: Some(0.15), opacity: 0.8, mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Titanium") },
        ],
    },
    SmartMaterial {
        name: "Terracotta Aged",
        layers: &[
            SmartLayer { part_var: 0.12, ..layer("Terracotta", "Terracotta") },
            SmartLayer {
                color: Some([0.45, 0.26, 0.17]),
                rough: Some(0.9),
                opacity: 0.7,
                mask: Some(SmartMask { invert: true, noise: Some(0.6), scale: Some(3.0), ..mix(GenKind::PartGradient, &[(WORLD_HEIGHT, 1.0)], 0.4, 0.15) }),
                ..layer("Water Soak", "Terracotta")
            },
            SmartLayer {
                color: Some([0.88, 0.86, 0.80]),
                rough: Some(0.95),
                height: Some(0.03),
                opacity: 0.75,
                mask: Some(SmartMask { scale: Some(3.5), noise: Some(0.8), ..mix(GenKind::Clouds, &[(NOISE, 0.8), (WORLD_HEIGHT, -0.5), (OCCLUSION, 0.4)], 0.42, 0.1) }),
                ..layer("Mineral Bloom", "Concrete")
            },
            SmartLayer {
                color: Some([0.82, 0.52, 0.36]),
                height: Some(-0.12),
                opacity: 0.85,
                mask: Some(CHIPS),
                ..layer("Chips", "Terracotta")
            },
            SmartLayer {
                color: Some([0.20, 0.28, 0.10]),
                height: Some(0.08),
                opacity: 0.85,
                mask: Some(SmartMask { noise: Some(0.6), scale: Some(5.0), ..mix(GenKind::CavityGrime, &[(CAVITY, 1.5), (OCCLUSION, 0.8), (NOISE, 0.4)], 0.65, 0.12) }),
                ..layer("Algae", "Moss")
            },
            GRIME_LAYER,
        ],
    },
    SmartMaterial {
        name: "Granite Snowy",
        layers: &[
            SmartLayer { part_var: 0.15, ..layer("Granite", "Granite") },
            SmartLayer {
                color: Some([0.12, 0.12, 0.12]),
                height: Some(-0.3),
                mask: Some(SmartMask { scale: Some(1.0), noise: Some(0.3), ..mix(GenKind::Clouds, &[(CELLS, 1.0)], 0.9, 0.04) }),
                ..layer("Fractures", "Granite")
            },
            SmartLayer {
                color: Some([0.20, 0.18, 0.16]),
                opacity: 0.7,
                channels: Some(COLOR_ONLY),
                mask: Some(mask(GenKind::RainStreaks)),
                ..layer("Meltwater Streaks", "Granite")
            },
            GRIME_LAYER,
            SmartLayer {
                color: Some([0.80, 0.86, 0.92]),
                rough: Some(0.15),
                opacity: 0.7,
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.5), ..mix(GenKind::Dust, &[(SKY, 0.8), (UP, 0.9), (OVERHANG, -0.5)], 0.8, 0.05) }),
                ..layer("Icy Melt Line", "Ice")
            },
            SmartLayer {
                height: Some(0.3),
                height_var: Some(0.25),
                var_scale: Some(8.0),
                mask: Some(SmartMask { scale: Some(3.0), noise: Some(0.5), ..mix(GenKind::Dust, &[(SKY, 0.8), (UP, 0.9), (OVERHANG, -0.5)], 0.95, 0.12) }),
                ..layer("Snow", "Snow")
            },
        ],
    },
    // ------------------------------------------------------------ scanned
    // Built on photo-scanned PBR sets (Poly Haven, CC0; assets/pbr): the
    // scan gives the base color, roughness, height and metalness, and
    // procedural layers add wear and dirt that follow the mesh.
    SmartMaterial {
        name: "Rusty Sheet Metal",
        layers: &[
            scan("Rusty Sheet", "Rust", "rusty_metal_02", 1.2, 0.2),
            SmartLayer {
                color: Some([0.30, 0.14, 0.06]),
                rough: Some(0.92),
                height: Some(0.05),
                opacity: 0.8,
                mask: Some(SmartMask { noise: Some(0.7), scale: Some(3.0), ..mix(GenKind::Dirt, &[(OCCLUSION, 2.0), (CAVITY, 1.5), (STREAKS, 0.6)], 0.55, 0.15) }),
                ..layer("Heavy Rust", "Rust")
            },
            SCAN_GRIME,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Diamond Plate Worn",
        layers: &[
            scan("Diamond Plate", "Steel Brushed", "metal_plate", 0.8, 0.5),
            SmartLayer { color: Some([0.72, 0.72, 0.73]), rough: Some(0.18), metal: Some(1.0), mask: Some(POLISHED_EDGES), ..layer("Polished Edges", "Steel Brushed") },
            SCAN_GRIME,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Corrugated Iron Weathered",
        layers: &[
            scan("Corrugated Iron", "Zinc Galvanized", "corrugated_iron", 1.0, 0.45),
            SmartLayer {
                color: Some([0.38, 0.19, 0.08]),
                rough: Some(0.9),
                metal: Some(0.0),
                opacity: 0.9,
                mask: Some(SmartMask { noise: Some(0.6), scale: Some(4.0), ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (STREAKS, 1.0), (CREASE, 1.0)], 0.6, 0.15) }),
                ..layer("Rust Runs", "Rust")
            },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Green Paint Rusted",
        layers: &[
            scan("Green Paint", "Paint Olive", "green_metal_rust", 1.2, 0.2),
            SmartLayer {
                color: Some([0.50, 0.50, 0.50]),
                rough: Some(0.35),
                metal: Some(1.0),
                height: Some(-0.04),
                mask: Some(CHIPS),
                ..layer("Bare Metal Chips", "Steel Brushed")
            },
            SCAN_GRIME,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Blue Painted Plate",
        layers: &[
            scan("Blue Paint", "Car Paint Blue", "blue_metal_plate", 1.0, 0.25),
            SmartLayer {
                color: Some([0.55, 0.55, 0.56]),
                rough: Some(0.3),
                metal: Some(1.0),
                height: Some(-0.03),
                mask: Some(SmartMask { edge_width: Some(0.008), ..CHIPS }),
                ..layer("Worn Edges", "Steel Brushed")
            },
            SmartLayer { color: Some([0.6, 0.6, 0.62]), rough: Some(0.3), metal: Some(1.0), opacity: 0.6, mask: Some(SCRATCHED), ..layer("Scratches", "Steel Brushed") },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Worn Table Wood",
        layers: &[
            scan("Table Wood", "Walnut", "wood_table_worn", 0.9, 0.2),
            SmartLayer {
                color: Some([0.62, 0.45, 0.28]),
                rough: Some(0.7),
                opacity: 0.6,
                mask: Some(SmartMask { edge_width: Some(0.01), noise: Some(0.6), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.6)], 0.6, 0.1) }),
                ..layer("Worn Edges", "Oak")
            },
            SCAN_GRIME,
        ],
    },
    SmartMaterial {
        name: "Oak Veneer",
        layers: &[scan("Oak Veneer", "Oak", "oak_veneer_01", 1.0, 0.08), DUST_LAYER],
    },
    SmartMaterial {
        name: "Weathered Planks",
        layers: &[
            scan("Planks", "Pine", "weathered_brown_planks", 0.7, 0.35),
            SmartLayer {
                color: Some([0.62, 0.60, 0.55]),
                rough: Some(0.95),
                opacity: 0.45,
                mask: Some(SmartMask { scale: Some(1.5), noise: Some(0.2), ..mix(GenKind::Clouds, &[(SKY, 0.8), (NOISE, 0.6)], 0.6, 0.25) }),
                ..layer("Sun Bleach", "Pine")
            },
            SmartLayer {
                color: Some([0.20, 0.27, 0.08]),
                rough: Some(0.95),
                height: Some(0.06),
                opacity: 0.85,
                mask: Some(SmartMask { noise: Some(0.8), scale: Some(5.0), ..mix(GenKind::Dirt, &[(OCCLUSION, 1.5), (CAVITY, 1.0), (UP, 0.5)], 0.65, 0.12) }),
                ..layer("Moss", "Fabric Olive")
            },
        ],
    },
    SmartMaterial {
        name: "Dark Wood Varnished",
        layers: &[
            scan("Dark Wood", "Walnut", "dark_wood", 0.8, 0.1),
            SmartLayer { rough: Some(0.18), opacity: 0.55, channels: Some(ROUGH_ONLY), ..layer("Varnish", "Walnut") },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Plywood",
        layers: &[
            scan("Plywood", "Pine", "plywood", 0.8, 0.12),
            SmartLayer {
                color: Some([0.28, 0.19, 0.10]),
                rough: Some(0.85),
                opacity: 0.5,
                mask: Some(SmartMask { edge_width: Some(0.012), noise: Some(0.4), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.4)], 0.55, 0.1) }),
                ..layer("Dark Edges", "Walnut")
            },
            SCAN_GRIME,
        ],
    },
    SmartMaterial {
        name: "Leather Brown Scan",
        layers: &[
            scan("Leather", "Leather Brown", "brown_leather", 2.0, 0.25),
            SmartLayer {
                color: Some([0.55, 0.36, 0.20]),
                rough: Some(0.45),
                opacity: 0.6,
                mask: Some(SmartMask { edge_width: Some(0.012), noise: Some(0.5), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.8)], 0.55, 0.12) }),
                ..layer("Polished Wear", "Leather Brown")
            },
            SCAN_GRIME,
        ],
    },
    SmartMaterial {
        name: "Denim",
        layers: &[
            scan("Denim", "Denim", "denim_fabric", 2.5, 0.2),
            SmartLayer {
                color: Some([0.62, 0.68, 0.76]),
                rough: Some(0.95),
                opacity: 0.55,
                mask: Some(SmartMask { edge_width: Some(0.02), noise: Some(0.6), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.8)], 0.5, 0.15) }),
                ..layer("Faded Edges", "Denim")
            },
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Linen",
        layers: &[scan("Linen", "Fabric Olive", "rough_linen", 2.0, 0.45), SCAN_GRIME],
    },
    SmartMaterial {
        name: "Concrete Worn",
        layers: &[
            scan("Concrete", "Concrete", "concrete_floor_worn_001", 0.8, 0.25),
            SmartLayer {
                color: Some([0.70, 0.69, 0.66]),
                rough: Some(0.9),
                opacity: 0.5,
                mask: Some(SmartMask { edge_width: Some(0.015), noise: Some(0.8), ..mix(GenKind::EdgeWear, &[(EDGE, 1.0), (CONVEX, 0.5), (NOISE, 0.3)], 0.6, 0.1) }),
                ..layer("Chipped Edges", "Concrete")
            },
            SCAN_GRIME,
            DUST_LAYER,
        ],
    },
    SmartMaterial {
        name: "Red Brick",
        layers: &[scan("Bricks", "Brick Red", "red_brick", 0.6, 0.4), SCAN_GRIME, DUST_LAYER],
    },
    SmartMaterial {
        name: "Castle Brick",
        layers: &[
            scan("Bricks", "Brick Red", "castle_brick_02_red", 0.6, 0.4),
            SmartLayer {
                color: Some([0.18, 0.26, 0.08]),
                rough: Some(0.95),
                height: Some(0.05),
                opacity: 0.8,
                mask: Some(SmartMask { noise: Some(0.8), scale: Some(5.0), ..mix(GenKind::Dirt, &[(CAVITY, 1.5), (OCCLUSION, 1.0), (SKY, 0.6)], 0.62, 0.12) }),
                ..layer("Moss", "Fabric Olive")
            },
            SCAN_GRIME,
        ],
    },
    SmartMaterial {
        name: "Rock Face",
        layers: &[
            scan("Rock", "Stone Dark", "rock_face_03", 0.5, 0.45),
            SmartLayer {
                color: Some([0.22, 0.30, 0.09]),
                rough: Some(0.95),
                height: Some(0.06),
                opacity: 0.9,
                mask: Some(SmartMask { noise: Some(0.8), scale: Some(4.0), ..mix(GenKind::Dust, &[(UP, 1.0), (SKY, 0.6), (CAVITY, 0.4)], 0.9, 0.2) }),
                ..layer("Moss On Top", "Fabric Olive")
            },
        ],
    },
    SmartMaterial {
        name: "Rustic Stone Wall",
        layers: &[scan("Stone Wall", "Stone Dark", "rustic_stone_wall_02", 0.5, 0.45), SCAN_GRIME, DUST_LAYER],
    },
    SmartMaterial {
        name: "Tree Bark",
        layers: &[
            scan("Bark", "Bark", "bark_brown_02", 0.8, 0.5),
            SmartLayer {
                color: Some([0.20, 0.28, 0.08]),
                rough: Some(0.95),
                opacity: 0.7,
                mask: Some(SmartMask { noise: Some(0.8), scale: Some(4.0), ..mix(GenKind::Dirt, &[(CAVITY, 1.5), (UP, 0.6)], 0.65, 0.15) }),
                ..layer("Moss", "Fabric Olive")
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_unique_and_layers_resolve() {
        for (i, m) in MATERIALS.iter().enumerate() {
            assert!(MATERIALS[..i].iter().all(|o| o.name != m.name), "duplicate material {}", m.name);
        }
        for (i, s) in SMART_MATERIALS.iter().enumerate() {
            assert!(SMART_MATERIALS[..i].iter().all(|o| o.name != s.name), "duplicate smart material {}", s.name);
            for l in s.layers {
                assert!(MATERIALS.iter().any(|m| m.name == l.material), "{}: unknown material {}", s.name, l.material);
                let _ = l.fill();
                let _ = l.generator();
            }
        }
    }
}

impl SmartLayer {
    pub fn fill(&self) -> FillParams {
        let mut f = MaterialPreset::by_name(self.material).fill();
        if let Some(c) = self.color {
            f.mat.color = c;
        }
        if let Some(r) = self.rough {
            f.mat.rough = r;
        }
        if let Some(m) = self.metal {
            f.mat.metal = m;
        }
        if let Some(h) = self.height {
            f.mat.height = h;
            f.mat.enabled[3] = true;
        }
        f.color_var = self.color_var.unwrap_or(f.color_var);
        f.rough_var = self.rough_var.unwrap_or(f.rough_var);
        f.var_scale = self.var_scale.unwrap_or(f.var_scale);
        if let Some(h) = self.height_var {
            f.height_var = h;
            f.mat.enabled[3] = true;
        }
        if let Some(ch) = self.channels {
            f.mat.enabled = ch;
        }
        f.part_var = self.part_var;
        f.seed = self.seed;
        if let Some(scan) = self.scan {
            f.projection = crate::engine::Projection::Triplanar;
            f.tiling = scan.tiling;
            // With a height image, the height value is its strength.
            f.mat.height = scan.height;
            f.mat.enabled = [true, true, true, true];
            f.color_var = 0.0;
            f.rough_var = 0.0;
            f.height_var = 0.0;
        }
        f
    }

    pub fn generator(&self) -> Option<Generator> {
        self.mask.map(|m| {
            let mut g = Generator { invert: m.invert, seed: m.seed, ..m.kind.default_generator() };
            if !m.weights.is_empty() {
                g.weights = [0.0; crate::engine::GEN_INPUT_COUNT];
                for &(i, w) in m.weights {
                    g.weights[i] = w;
                }
            }
            g.level = m.level.unwrap_or(g.level);
            g.contrast = m.contrast.unwrap_or(g.contrast);
            g.scale = m.scale.unwrap_or(g.scale);
            g.noise = m.noise.unwrap_or(g.noise);
            g.edge_width = m.edge_width.unwrap_or(g.edge_width);
            g
        })
    }
}

// ---------------------------------------------------------------- previews

fn hash3(p: [i32; 3]) -> f32 {
    let mut h = (p[0] as u32).wrapping_mul(0x8da6b343)
        ^ (p[1] as u32).wrapping_mul(0xd8163841)
        ^ (p[2] as u32).wrapping_mul(0xcb1ab31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h & 0xffffff) as f32 / 16777215.0
}

fn vnoise(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let ii = [i.x as i32, i.y as i32, i.z as i32];
    let h = |x: i32, y: i32, z: i32| hash3([ii[0] + x, ii[1] + y, ii[2] + z]);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(
        lerp(lerp(h(0, 0, 0), h(1, 0, 0), u.x), lerp(h(0, 1, 0), h(1, 1, 0), u.x), u.y),
        lerp(lerp(h(0, 0, 1), h(1, 0, 1), u.x), lerp(h(0, 1, 1), h(1, 1, 1), u.x), u.y),
        u.z,
    )
}

fn fbm(p: Vec3) -> f32 {
    let (mut s, mut a, mut q) = (0.0, 0.5, p);
    for _ in 0..4 {
        s += a * vnoise(q);
        q = q * 2.03 + Vec3::new(1.7, 9.2, 3.1);
        a *= 0.5;
    }
    s / 0.9375
}

/// Straight tapered scratches on a plane, as `scratch_plane` in composite.wgsl.
fn scratch_plane(x: f32, y: f32, seed: f32) -> f32 {
    let (cx, cy) = (x.floor(), y.floor());
    let s = (seed * 1000.0) as i32;
    let mut m = 0.0f32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let c = [cx as i32 + dx, cy as i32 + dy, s];
            let (hx, hy, hz) = (hash3(c), hash3([c[0] + 31, c[1], s]), hash3([c[0], c[1] + 57, s]));
            if hz < 0.6 {
                continue;
            }
            let ang = (hz * 7.31).fract() * std::f32::consts::PI;
            let (dirx, diry) = (ang.cos(), ang.sin());
            let half_len = 0.5 + 0.9 * (hx * 5.17).fract();
            let (rx, ry) = (x - (c[0] as f32 + hx), y - (c[1] as f32 + hy));
            let t = rx * dirx + ry * diry;
            if t.abs() > half_len {
                continue;
            }
            let d = ((rx - dirx * t).powi(2) + (ry - diry * t).powi(2)).sqrt();
            let width = 0.025 + 0.035 * (hy * 3.71).fract();
            let taper = 1.0 - smoothstep(0.55, 1.0, t.abs() / half_len);
            m = m.max((1.0 - smoothstep(width * 0.4, width, d)) * taper);
        }
    }
    m
}

/// Distance to the nearest jittered feature point, as `cellular` in common.wgsl.
fn cellular(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let mut d = 8.0f32;
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                let c = [i.x as i32 + x, i.y as i32 + y, i.z as i32 + z];
                let jitter = Vec3::new(hash3(c), hash3([c[0] + 31, c[1], c[2]]), hash3([c[0], c[1] + 57, c[2]]));
                let r = Vec3::new(x as f32, y as f32, z as f32) + jitter - f;
                d = d.min(r.length_squared());
            }
        }
    }
    d.sqrt()
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Approximates a smart mask on a preview sphere, which has no baked maps:
/// the rim stands in for edges, the underside for occlusion.
fn preview_mask(g: &Generator, p: Vec3, n: Vec3) -> f32 {
    let q = p * g.scale + Vec3::splat(g.seed * 17.31);
    let rim = 1.0 - n.z;
    let occ = (0.5 - 0.5 * n.y).clamp(0.0, 1.0) * 0.6;
    let mut inp = [0.0f32; crate::engine::GEN_INPUT_COUNT];
    inp[EDGE] = smoothstep(0.55, 0.85, rim);
    inp[CONVEX] = rim * rim * 0.6;
    inp[OCCLUSION] = occ;
    inp[CAVITY] = occ * 0.4;
    inp[SKY] = n.y.max(0.0);
    inp[UP] = n.y.max(0.0);
    inp[THIN] = smoothstep(0.7, 1.0, rim);
    inp[PART_GRADIENT] = (p.y + 1.0) * 0.5;
    inp[WORLD_HEIGHT] = (p.y + 1.0) * 0.5;
    inp[PART_RANDOM] = 0.5;
    inp[STREAKS] = smoothstep(0.45, 0.8, fbm(Vec3::new(q.x * 7.0, q.y * 0.45, q.z * 7.0)));
    inp[NOISE] = fbm(q);
    inp[OVERHANG] = (-n.y).max(0.0);
    inp[GRAIN] = smoothstep(0.35, 0.75, fbm(Vec3::new(q.x * 0.5, q.y * 30.0, q.z * 30.0)));
    inp[CELLS] = cellular(q * 4.0);
    inp[SCRATCHES] = scratch_plane(q.x * 5.0, q.y * 5.0, g.seed);
    let mut v: f32 = g.weights.iter().zip(inp).map(|(w, x)| w * x).sum();
    v += (fbm(q * 2.0) - 0.5) * g.noise;
    let m = smoothstep(g.level - g.contrast, g.level + g.contrast, v);
    if g.invert { 1.0 - m } else { m }
}

pub struct PreviewLayer {
    pub fill: FillParams,
    pub opacity: f32,
    pub generator: Option<Generator>,
}

const PREVIEW: usize = 72;

/// Renders a small lit sphere for a stack of fill layers.
pub fn render_preview(layers: &[PreviewLayer]) -> egui::ColorImage {
    let mut px = vec![0u8; PREVIEW * PREVIEW * 4];
    let l = Vec3::new(-0.45, 0.6, 0.65).normalize();
    for y in 0..PREVIEW {
        for x in 0..PREVIEW {
            let u = (x as f32 + 0.5) / PREVIEW as f32 * 2.0 - 1.0;
            let v = 1.0 - (y as f32 + 0.5) / PREVIEW as f32 * 2.0;
            let r2 = u * u + v * v;
            let i = (y * PREVIEW + x) * 4;
            if r2 > 1.0 {
                continue;
            }
            let n = Vec3::new(u, v, (1.0 - r2).sqrt());
            let (mut color, mut rough, mut metal) = (Vec3::splat(0.75), 0.5f32, 0.0f32);
            for layer in layers {
                let f = &layer.fill;
                let p = n * 1.0;
                let nv = fbm(p * f.var_scale + Vec3::splat(f.seed * 13.7));
                let mut c = Vec3::from_array(f.mat.color) * (1.0 + (nv - 0.5) * 2.0 * f.color_var);
                c = c.clamp(Vec3::ZERO, Vec3::ONE);
                let r = (f.mat.rough + (nv - 0.5) * f.rough_var).clamp(0.0, 1.0);
                let a = layer.opacity * layer.generator.as_ref().map_or(1.0, |g| preview_mask(g, p, n));
                let en = f.mat.enabled;
                if en[0] {
                    color = color.lerp(c, a);
                }
                if en[1] {
                    rough += (r - rough) * a;
                }
                if en[2] {
                    metal += (f.mat.metal - metal) * a;
                }
            }
            let albedo = Vec3::new(srgb_to_linear(color.x), srgb_to_linear(color.y), srgb_to_linear(color.z));
            let view = Vec3::Z;
            let h = (l + view).normalize();
            let nl = n.dot(l).max(0.0);
            let nh = n.dot(h).max(0.0);
            let a2 = (rough * rough).max(0.002).powi(2);
            let d = a2 / (std::f32::consts::PI * (nh * nh * (a2 - 1.0) + 1.0).powi(2));
            let f0 = Vec3::splat(0.04).lerp(albedo, metal);
            let fres = f0 + (Vec3::ONE - f0) * (1.0 - n.dot(view).max(0.0)).powi(5) * (1.0 - rough);
            let spec = fres * d * 0.25 * nl;
            let diffuse = albedo * (1.0 - metal) * (nl * 0.9 + 0.25);
            // Environment reflection: bright top, dark bottom.
            let rdir = (2.0 * n.dot(view) * n - view).normalize();
            let env = 0.25 + 0.55 * (rdir.y * 0.5 + 0.5);
            let refl = fres * env * (1.0 - rough * 0.6) + f0 * metal * 0.35;
            let lin = diffuse + spec * 1.8 + refl;
            let mapped = lin / (lin + Vec3::ONE) * 1.6;
            px[i] = (linear_to_srgb(mapped.x) * 255.0) as u8;
            px[i + 1] = (linear_to_srgb(mapped.y) * 255.0) as u8;
            px[i + 2] = (linear_to_srgb(mapped.z) * 255.0) as u8;
            // Soft anti-aliased silhouette.
            px[i + 3] = (((1.0 - r2.sqrt()) * PREVIEW as f32 * 0.5).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([PREVIEW, PREVIEW], &px)
}

/// Grayscale sphere showing a generator's mask.
pub fn render_generator_preview(kind: GenKind) -> egui::ColorImage {
    let g = kind.default_generator();
    let white = PreviewLayer {
        fill: FillParams {
            mat: MatValues { enabled: [true, true, true, false], color: [0.92, 0.92, 0.9], rough: 0.6, metal: 0.0, height: 0.0 },
            ..Default::default()
        },
        opacity: 1.0,
        generator: Some(g),
    };
    let black = PreviewLayer {
        fill: FillParams {
            mat: MatValues { enabled: [true, true, true, false], color: [0.06, 0.06, 0.06], rough: 0.6, metal: 0.0, height: 0.0 },
            ..Default::default()
        },
        opacity: 1.0,
        generator: None,
    };
    render_preview(&[black, white])
}

/// What a material is, for sorting and filtering the shelf.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum MaterialType {
    Paint,
    Metal,
    Wood,
    Stone,
    Fabric,
    Plastic,
    Glass,
    Nature,
    Other,
}

impl MaterialType {
    pub const ALL: [MaterialType; 9] = [
        MaterialType::Paint,
        MaterialType::Metal,
        MaterialType::Wood,
        MaterialType::Stone,
        MaterialType::Fabric,
        MaterialType::Plastic,
        MaterialType::Glass,
        MaterialType::Nature,
        MaterialType::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MaterialType::Paint => "Paint & coatings",
            MaterialType::Metal => "Metal",
            MaterialType::Wood => "Wood",
            MaterialType::Stone => "Stone & concrete",
            MaterialType::Fabric => "Fabric, leather & paper",
            MaterialType::Plastic => "Plastic, rubber & wax",
            MaterialType::Glass => "Glass & ceramic",
            MaterialType::Nature => "Nature & ground",
            MaterialType::Other => "Other",
        }
    }

    /// From a material's name (painted surfaces count as paint: "Painted Steel" is paint
    /// first); `metal` (its metallic value) decides for names that say nothing.
    pub fn of(name: &str, metal: Option<f32>) -> MaterialType {
        const WORDS: [(MaterialType, &[&str]); 8] = [
            (MaterialType::Paint, &["paint", "lacquer", "enamel", "coating", "powder coat"]),
            (
                MaterialType::Metal,
                &["metal", "steel", "iron", "rust ", "rusty", "rusted", "copper", "brass", "bronze", "gold", "silver", "alumin", "chrome", "tin ", "zinc", "nickel", "titanium", "galvan", "pewter"],
            ),
            (MaterialType::Wood, &["wood", "oak", "pine", "walnut", "plank", "bark", "birch", "maple", "bamboo", "plywood", "mahogany", "ebony", "teak", "cedar", "cherry"]),
            (
                MaterialType::Stone,
                &["stone", "concrete", "brick", "marble", "granite", "rock", "cobble", "tile", "slate", "asphalt", "plaster", "cement", "terrazzo", "pavement", "gravel", "clay", "obsidian", "jade"],
            ),
            (MaterialType::Fabric, &["fabric", "cloth", "leather", "denim", "cotton", "wool", "silk", "carpet", "canvas", "felt", "velvet", "linen", "suede", "paper", "cardboard"]),
            (MaterialType::Plastic, &["plastic", "rubber", "vinyl", "silicone", "acrylic", "polymer", "foam", "carbon fiber", "wax"]),
            (MaterialType::Glass, &["glass", "ceramic", "porcelain", "crystal", "terracotta"]),
            (MaterialType::Nature, &["ground", "soil", "dirt", "mud", "sand", "grass", "moss", "snow", "ice", "leaf", "leaves", "bone", "skin", "organic", "forest", "rock"]),
        ];
        let n = format!("{} ", name.to_lowercase());
        for (kind, words) in WORDS {
            if words.iter().any(|w| n.contains(w)) {
                return kind;
            }
        }
        match metal {
            Some(m) if m > 0.5 => MaterialType::Metal,
            _ => MaterialType::Other,
        }
    }
}

impl MaterialPreset {
    pub fn kind(&self) -> MaterialType {
        MaterialType::of(self.name, Some(self.metal))
    }
}

impl SmartMaterial {
    /// From its name, or else from the material its first layer is made of.
    pub fn kind(&self) -> MaterialType {
        match MaterialType::of(self.name, None) {
            MaterialType::Other => self
                .layers
                .first()
                .and_then(|l| MATERIALS.iter().find(|m| m.name == l.material))
                .map_or(MaterialType::Other, |m| m.kind()),
            k => k,
        }
    }
}

#[cfg(test)]
mod type_tests {
    use super::*;

    /// Shelf types read from names: a few known ones, and the whole list printed for review.
    #[test]
    fn material_types() {
        assert_eq!(MaterialType::of("Painted Steel Worn", None), MaterialType::Paint);
        assert_eq!(MaterialType::of("Rusty Sheet Metal", None), MaterialType::Metal);
        assert_eq!(MaterialType::of("Oak Aged", None), MaterialType::Wood);
        assert_eq!(MaterialType::of("Concrete Smooth", None), MaterialType::Stone);
        assert_eq!(MaterialType::of("Mystery", Some(1.0)), MaterialType::Metal);
        assert_eq!(MaterialType::of("Rustic Stone Wall", None), MaterialType::Stone, "rustic isn't rust");
        assert_eq!(MaterialType::of("Walnut Varnished", None), MaterialType::Wood, "varnished wood is wood");
        let mut other = Vec::new();
        for m in MATERIALS {
            println!("  material {:<28} {:?}", m.name, m.kind());
            if m.kind() == MaterialType::Other {
                other.push(m.name);
            }
        }
        for s in SMART_MATERIALS {
            println!("  smart    {:<28} {:?}", s.name, s.kind());
            if s.kind() == MaterialType::Other {
                other.push(s.name);
            }
        }
        println!("  other: {other:?}");
    }
}
