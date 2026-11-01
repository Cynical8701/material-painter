//! Library of procedural brush alphas (stamp tips).
//!
//! Each tip is a centred grayscale image (0 = no paint, 255 = full paint)
//! evaluated in continuous `[-1,1]²` space, so any resolution shows the same
//! shape. Everything fades to zero well inside the unit circle the stamp is
//! sampled in. Directional tips (scratches, bristles, slots, stitches) run
//! along +X so rotating to the stroke direction works.
//!
//! The "height" tips (Dome, Rivet, Hex Bolt, ...) are smooth profiles meant
//! for painting into the height channel.

use crate::textures::{aa, bevel, lerp, par_pixels, pnoise, rand01, sd_round_box, seg_dist, smoothstep, to_u8, voronoi};
use std::f32::consts::{PI, TAU};

/// A named procedural brush tip.
pub struct BrushAlpha {
    pub name: &'static str,
    generate: fn(u32) -> Vec<f32>,
}

impl BrushAlpha {
    /// Renders the tip as `size × size` row-major 8-bit grayscale.
    pub fn render(&self, size: u32) -> Vec<u8> {
        let size = size.max(2);
        let mut v = (self.generate)(size);
        // Safety falloff: the stamp is sampled inside the unit circle.
        let s = size as usize;
        for y in 0..s {
            for x in 0..s {
                let (px, py) = (coord(x, size), coord(y, size));
                let r = (px * px + py * py).sqrt();
                v[y * s + x] *= 1.0 - smoothstep(0.86, 0.95, r);
            }
        }
        to_u8(v)
    }
}

const fn tip(name: &'static str, generate: fn(u32) -> Vec<f32>) -> BrushAlpha {
    BrushAlpha { name, generate }
}

pub const BRUSH_ALPHAS: &[BrushAlpha] = &[
    tip("Square", square),
    tip("Spray", spray),
    tip("Grunge", grunge),
    tip("Splatter", splatter),
    tip("Chalk", chalk),
    tip("Scratches", scratches),
    tip("Bristles", bristles),
    tip("Cracks", cracks),
    tip("Stipple", stipple),
    tip("Fabric", fabric),
    tip("Star", star),
    tip("Leaf", leaf),
    tip("Dome", dome),
    tip("Rivet", rivet),
    tip("Hex Bolt", hex_bolt),
    tip("Phillips Screw", phillips_screw),
    tip("Slotted Screw", slotted_screw),
    tip("Vent Slot", vent_slot),
    tip("Stitch", stitch),
    tip("Double Stitch", double_stitch),
    tip("Cross Stitch", cross_stitch),
    tip("Slanted Stitch", slanted_stitch),
    tip("Ring", ring),
    tip("Washer", washer),
];

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

#[inline]
fn coord(i: usize, size: u32) -> f32 {
    (i as f32 + 0.5) / size as f32 * 2.0 - 1.0
}

/// Evaluates `f(x, y, px)` over `[-1,1]²` where `px` is the pixel footprint.
fn pixels(size: u32, f: impl Fn(f32, f32, f32) -> f32 + Sync) -> Vec<f32> {
    let px = 2.0 / size as f32;
    par_pixels(size, |u, v| f(u * 2.0 - 1.0, v * 2.0 - 1.0, px))
}

const BIG: i32 = 1 << 16;

/// Non-periodic gradient noise (~[-1,1]).
#[inline]
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    pnoise(x + 512.0, y + 512.0, BIG, BIG, seed)
}

fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0);
    for o in 0..octaves {
        sum += amp * noise(x * f, y * f, seed.wrapping_add(o * 977));
        norm += amp;
        amp *= 0.5;
        f *= 2.03;
    }
    sum / norm * 1.5
}

#[inline]
fn rnd(i: i32, k: i32, seed: u32) -> f32 {
    rand01(i, k, seed)
}

/// Max-blended splat canvas in `[-1,1]²` coordinates.
struct Canvas {
    s: usize,
    px: f32,
    data: Vec<f32>,
}

impl Canvas {
    fn new(size: u32) -> Self {
        let s = size as usize;
        Self { s, px: 2.0 / size as f32, data: vec![0.0; s * s] }
    }

    fn from(size: u32, data: Vec<f32>) -> Self {
        Self { s: size as usize, px: 2.0 / size as f32, data }
    }

    fn bbox(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> (usize, usize, usize, usize) {
        let s = self.s as f32;
        let to = |c: f32| ((c + 1.0) * 0.5 * s - 0.5).clamp(0.0, s - 1.0);
        (to(x0).floor() as usize, to(y0).floor() as usize, to(x1).ceil() as usize, to(y1).ceil() as usize)
    }

    /// Anti-aliased disc. Discs smaller than a pixel keep their energy by
    /// growing to pixel size and fading.
    fn disc(&mut self, cx: f32, cy: f32, r: f32, k: f32) {
        let re = r.max(self.px * 0.6);
        let k = k * (r / re).powi(2);
        let m = re + self.px * 2.0;
        let (x0, y0, x1, y1) = self.bbox(cx - m, cy - m, cx + m, cy + m);
        let size = self.s as u32;
        for y in y0..=y1 {
            let py = coord(y, size);
            for x in x0..=x1 {
                let pxx = coord(x, size);
                let d = ((pxx - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let c = aa(re - d, self.px) * k;
                let o = &mut self.data[y * self.s + x];
                *o = o.max(c);
            }
        }
    }

    /// Line with round caps whose radius goes from `w0` to `w1` along it and
    /// is additionally scaled by `profile(t)`.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), w0: f32, w1: f32, k: f32, profile: impl Fn(f32) -> f32) {
        let wm = w0.max(w1).max(self.px * 0.6);
        let m = wm + self.px * 2.0;
        let (x0, y0, x1, y1) = self.bbox(a.0.min(b.0) - m, a.1.min(b.1) - m, a.0.max(b.0) + m, a.1.max(b.1) + m);
        let size = self.s as u32;
        for y in y0..=y1 {
            let py = coord(y, size);
            for x in x0..=x1 {
                let pxx = coord(x, size);
                let (d, t) = seg_dist(pxx, py, a.0, a.1, b.0, b.1);
                let w = lerp(w0, w1, t) * profile(t);
                let we = w.max(self.px * 0.6);
                let c = aa(we - d, self.px) * k * (w / we).min(1.0);
                let o = &mut self.data[y * self.s + x];
                *o = o.max(c);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// paint tips
// ---------------------------------------------------------------------------

fn square(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let sd = sd_round_box(x, y, 0.6, 0.6, 0.12);
        smoothstep(-0.5 * px, 0.04 + px, -sd)
    })
}

fn spray(size: u32) -> Vec<f32> {
    let mut c = Canvas::new(size);
    let seed = 1001;
    let mut placed = 0;
    let mut i = 0;
    while placed < 1500 {
        i += 1;
        // gaussian radius: dense core, sparse fringe
        let r = 0.3 * (-2.0 * (1.0 - rnd(i, 0, seed)).ln()).sqrt();
        if r > 0.8 {
            continue;
        }
        placed += 1;
        let a = rnd(i, 1, seed) * TAU;
        // At least about a pixel across, so small renders keep their dots.
        let dr = (0.004 + 0.014 * rnd(i, 2, seed).powi(3)).max(1.2 / size as f32);
        let k = 0.5 + 0.5 * rnd(i, 3, seed);
        c.disc(a.cos() * r, a.sin() * r, dr, k);
    }
    c.data
}

fn grunge(size: u32) -> Vec<f32> {
    pixels(size, |x, y, _| {
        let r = (x * x + y * y).sqrt();
        let n = fbm(x * 2.2, y * 2.2, 6, 1101) * 0.5 + 0.5;
        let fall = 1.0 - smoothstep(0.25, 0.82, r);
        let t = n * 0.7 + fall * 0.55;
        let mass = smoothstep(0.62, 0.78, t);
        let grain = fbm(x * 14.0, y * 14.0, 3, 1103) * 0.5 + 0.5;
        mass * (0.55 + 0.45 * smoothstep(0.25, 0.65, grain))
    })
}

fn splatter(size: u32) -> Vec<f32> {
    let seed = 1201;
    let blob_r = |a: f32| {
        let (c, s) = (a.cos(), a.sin());
        0.34 + 0.08 * fbm(c * 1.3 + 3.0, s * 1.3 + 3.0, 4, seed) + 0.03 * noise(c * 6.0, s * 6.0, seed + 7)
    };
    let base = pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let a = y.atan2(x);
        aa(blob_r(a) - r, px)
    });
    let mut c = Canvas::from(size, base);
    // satellite droplets
    for i in 0..46 {
        let a = rnd(i, 0, seed) * TAU;
        let edge = blob_r(a);
        let dist = edge + 0.03 + 0.42 * rnd(i, 1, seed).powf(1.4);
        if dist > 0.8 {
            continue;
        }
        let falloff = 1.0 - ((dist - edge) / (0.82 - edge)).clamp(0.0, 1.0);
        let r = 0.006 + 0.05 * falloff.powf(1.5) * rnd(i, 2, seed);
        c.disc(a.cos() * dist, a.sin() * dist, r, 1.0);
    }
    // radial streaks thrown from the blob
    for i in 0..9 {
        let a = rnd(i, 5, seed) * TAU;
        let edge = blob_r(a) - 0.04;
        let len = 0.12 + 0.25 * rnd(i, 6, seed);
        let end = (edge + len).min(0.78);
        let w = 0.02 + 0.025 * rnd(i, 7, seed);
        c.line((a.cos() * edge, a.sin() * edge), (a.cos() * end, a.sin() * end), w, w * 0.15, 1.0, |_| 1.0);
        // droplet at the tip of the streak
        let tip = end + 0.03;
        if tip < 0.8 {
            c.disc(a.cos() * tip, a.sin() * tip, w * 0.6, 1.0);
        }
    }
    c.data
}

fn chalk(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let edge_r = 0.66 + 0.05 * fbm(x * 3.0, y * 3.0, 3, 1301) + 0.035 * fbm(x * 14.0, y * 14.0, 2, 1303);
        let shape = smoothstep(-px, 0.015 + px, edge_r - r);
        let tooth = fbm(x * 22.0, y * 22.0, 3, 1305);
        let depth = (edge_r - r).max(0.0) * 1.4; // grain fills in towards the centre
        let fill = smoothstep(-0.45, 0.15, tooth + depth);
        shape * (0.15 + 0.85 * fill)
    })
}

fn scratches(size: u32) -> Vec<f32> {
    let mut c = Canvas::new(size);
    let seed = 1401;
    for i in 0..13 {
        let yc = (rnd(i, 0, seed) - 0.5) * 1.0;
        let half_room = (0.76f32.powi(2) - yc * yc).max(0.04).sqrt();
        let len = half_room * (0.7 + 0.6 * rnd(i, 1, seed));
        let xc = (rnd(i, 2, seed) - 0.5) * (2.0 * half_room - len).max(0.0);
        let ang = (rnd(i, 3, seed) - 0.5) * 0.18;
        let bend = (rnd(i, 4, seed) - 0.5) * 0.08;
        let w = 0.004 + 0.012 * rnd(i, 5, seed).powi(2);
        let k = 0.45 + 0.55 * rnd(i, 6, seed);
        // slightly curved, tapered at both ends, drawn as a short polyline
        let pts = 10;
        let point = |t: f32| {
            let s = (t - 0.5) * len;
            let off = bend * (1.0 - 4.0 * (t - 0.5) * (t - 0.5));
            let (ca, sa) = (ang.cos(), ang.sin());
            (xc + s * ca - off * sa, yc + s * sa + off * ca)
        };
        for p in 0..pts {
            let t0 = p as f32 / pts as f32;
            let t1 = (p + 1) as f32 / pts as f32;
            let taper = |t: f32| (4.0 * t * (1.0 - t)).max(0.0).powf(0.6);
            c.line(point(t0), point(t1), w * taper(t0), w * taper(t1), k, |_| 1.0);
        }
    }
    c.data
}

fn bristles(size: u32) -> Vec<f32> {
    let seed = 1501;
    let count = 34;
    let span = 1.1;
    pixels(size, |x, y, px| {
        let f = (y + span * 0.5) / span * count as f32 - 0.5;
        let b0 = f.floor() as i32;
        let mut v: f32 = 0.0;
        for b in b0 - 1..=b0 + 2 {
            if b < 0 || b >= count {
                continue;
            }
            let yc = (b as f32 + 0.5) / count as f32 * span - span * 0.5 + (rnd(b, 0, seed) - 0.5) * 0.02;
            let w = span / count as f32 * (0.25 + 0.3 * rnd(b, 1, seed));
            let across = aa(w - (y - yc).abs(), px) * (1.0 - ((y - yc).abs() / w).min(1.0) * 0.4);
            // bristle extent follows an elliptical brush footprint
            let room = (0.74f32.powi(2) - yc * yc).max(0.0).sqrt();
            let x0 = -room + 0.18 * rnd(b, 2, seed);
            let x1 = room - 0.3 * rnd(b, 3, seed);
            let along = smoothstep(x0, x0 + 0.12, x) * (1.0 - smoothstep(x1 - 0.35, x1, x));
            // dry-brush gaps
            let gap = smoothstep(-0.35, 0.05, noise(x * 4.0, b as f32 * 3.7, seed + 5) + 0.25 * (1.0 + x));
            let k = 0.55 + 0.45 * rnd(b, 4, seed);
            v = v.max(across * along * gap * k);
        }
        v
    })
}

fn cracks(size: u32) -> Vec<f32> {
    let seed = 1601;
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let wx = x + fbm(x * 2.0, y * 2.0, 3, seed) * 0.06;
        let wy = y + fbm(x * 2.0, y * 2.0, 3, seed + 1) * 0.06;
        let s = 2.6;
        let c = voronoi(wx * s + 100.0, wy * s + 100.0, BIG, 0.9, seed + 2);
        let fall = 1.0 - smoothstep(0.2, 0.78, r);
        let w = (0.008 + 0.012 * (noise(x * 4.0, y * 4.0, seed + 3) * 0.5 + 0.5)) * s * fall;
        let main = aa(w - c.border, px * s);
        let s2 = 6.0;
        let c2 = voronoi(wx * s2 + 50.0, wy * s2 + 50.0, BIG, 0.9, seed + 4);
        let keep = smoothstep(0.0, 0.3, noise(x * 3.0, y * 3.0, seed + 5)) * fall;
        let fine = aa(0.008 * s2 * keep - c2.border, px * s2) * 0.7;
        main.max(fine)
    })
}

fn stipple(size: u32) -> Vec<f32> {
    let mut c = Canvas::new(size);
    let seed = 1701;
    let n = 16;
    for j in 0..n {
        for i in 0..n {
            let cell = 1.6 / n as f32;
            let x = -0.8 + (i as f32 + 0.15 + 0.7 * rnd(i, j, seed)) * cell;
            let y = -0.8 + (j as f32 + 0.15 + 0.7 * rnd(i, j, seed + 1)) * cell;
            let r = (x * x + y * y).sqrt();
            let density = 1.0 - smoothstep(0.2, 0.78, r);
            if rnd(i, j, seed + 2) > density {
                continue;
            }
            let dr = 0.015 + 0.022 * rnd(i, j, seed + 3);
            c.disc(x, y, dr, 0.7 + 0.3 * rnd(i, j, seed + 4));
        }
    }
    c.data
}

fn fabric(size: u32) -> Vec<f32> {
    pixels(size, |x, y, _| {
        let r = (x * x + y * y).sqrt();
        let k = 7.0;
        let (gx, gy) = ((x + 1.0) * k, (y + 1.0) * k);
        let (i, j) = (gx.floor() as i32, gy.floor() as i32);
        let (fx, fy) = (gx.fract(), gy.fract());
        let bump = |f: f32| (1.0 - ((f - 0.5) * 2.0 / 0.85).powi(2)).max(0.0).sqrt();
        let arc = |f: f32| (f * PI).sin();
        let hh = bump(fy);
        let hv = bump(fx);
        let w = if (i + j) & 1 == 0 { (hh * (0.6 + 0.4 * arc(fx))).max(hv * 0.45) } else { (hv * (0.6 + 0.4 * arc(fy))).max(hh * 0.45) };
        let fall = 1.0 - smoothstep(0.35, 0.8, r + fbm(x * 3.0, y * 3.0, 3, 1801) * 0.08);
        w * fall
    })
}

fn star(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        // iq's sdStar5, pointing up (image y is down)
        let (k1x, k1y) = (0.809_017, -0.587_785);
        let (k2x, k2y) = (-k1x, k1y);
        let (mut px_, mut py_) = (x.abs(), -y);
        let d1 = (k1x * px_ + k1y * py_).max(0.0) * 2.0;
        px_ -= d1 * k1x;
        py_ -= d1 * k1y;
        let d2 = (k2x * px_ + k2y * py_).max(0.0) * 2.0;
        px_ -= d2 * k2x;
        py_ -= d2 * k2y;
        px_ = px_.abs();
        let r = 0.74;
        let rf = 0.45;
        py_ -= r;
        let (bax, bay) = (rf * -k1y, rf * k1x - 1.0);
        let h = ((px_ * bax + py_ * bay) / (bax * bax + bay * bay)).clamp(0.0, r);
        let (ex, ey) = (px_ - bax * h, py_ - bay * h);
        let sd = (ex * ex + ey * ey).sqrt() * (py_ * bax - px_ * bay).signum() - 0.02;
        smoothstep(-0.5 * px, 0.025 + px, -sd)
    })
}

fn leaf(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let (x0, x1) = (-0.62, 0.78);
        let t = (x - x0) / (x1 - x0);
        let c = 0.05 * (t * PI).sin();
        let dy = y - c;
        let mut v: f32 = 0.0;
        if (0.0..=1.0).contains(&t) {
            let w = 0.3 * (t.powf(0.8) * PI).sin().max(0.0).powf(0.9);
            let sd = w - dy.abs();
            let body = aa(sd * 0.9, px);
            // veins: midrib + angled side veins
            let mid = aa(0.006 + 0.008 * (1.0 - t) - dy.abs(), px);
            let ph = (t * 8.0 - dy.abs() * 5.0).rem_euclid(1.0);
            let side = aa(0.05 - (ph - 0.5).abs(), px * 8.0) * smoothstep(0.03, 0.1, w - dy.abs()) * smoothstep(0.0, 0.03, dy.abs());
            let shade = 0.88 + 0.12 * (sd / 0.3).clamp(0.0, 1.0);
            v = body * shade * (1.0 - 0.45 * mid.max(side * 0.7));
        }
        // stem
        let (d, _) = seg_dist(x, y, -0.8, 0.02, x0 + 0.02, 0.0);
        v.max(aa(0.012 - d, px))
    })
}

// ---------------------------------------------------------------------------
// height tips
// ---------------------------------------------------------------------------

fn dome(size: u32) -> Vec<f32> {
    pixels(size, |x, y, _| {
        let r = (x * x + y * y).sqrt() / 0.8;
        (1.0 - r * r).max(0.0).sqrt()
    })
}

fn rivet(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let flange = 0.22 * bevel(0.74 - r, 0.05, px);
        let head_r = 0.5;
        let t = (r / head_r).min(1.0);
        let head = (0.22 + 0.78 * (1.0 - t * t).sqrt()) * aa(head_r - r, px);
        flange.max(head)
    })
}

fn hex_bolt(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let washer = 0.28 * bevel(0.8 - r, 0.04, px);
        // flat-top hexagon (flats on top/bottom) with circumradius R
        let rr = 0.64;
        let (ax, ay) = (x.abs(), y.abs());
        let hexd = ay.max(0.5 * ay + 0.866_025_4 * ax);
        let inside = rr * 0.866_025_4 - hexd;
        // conical chamfer rounds off the corners of the head
        let chamfer = ((0.67 - r) / 0.12).clamp(0.0, 1.0);
        let top = 0.3 + 0.62 * chamfer.sqrt() + 0.04 * (1.0 - r / rr).max(0.0);
        let head = top * bevel(inside, 0.03, px);
        washer.max(head)
    })
}

/// Pan-head screw profile: flat top, rounded rim.
#[inline]
fn pan_head(r: f32, radius: f32, px: f32) -> f32 {
    let t = (r / radius).min(1.0);
    0.9 * (1.0 - t.powi(4)).max(0.0).sqrt() * aa(radius - r, px).max(0.0)
}

fn phillips_screw(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let head = pan_head(r, 0.74, px);
        let l = 0.34;
        let arm = |a: f32, b: f32| {
            let w = lerp(0.075, 0.045, (a.abs() / l).min(1.0));
            (b.abs() - w).max(a.abs() - l)
        };
        let sd = arm(x, y).min(arm(y, x));
        let m = smoothstep(-0.5 * px, 0.02 + px, -sd);
        // conical bit recess: deepest at the centre
        let depth = 0.12 + 0.55 * (x.abs().max(y.abs()) / l).min(1.0);
        lerp(head, head.min(depth), m)
    })
}

fn slotted_screw(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let head = pan_head(r, 0.74, px);
        let sd = y.abs() - 0.065;
        let m = smoothstep(-0.5 * px, 0.015 + px, -sd);
        lerp(head, head.min(0.18), m)
    })
}

fn vent_slot(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let qx = (x.abs() - 0.52).max(0.0);
        let sd = (qx * qx + y * y).sqrt() - 0.2;
        let t = bevel(-sd, 0.11, px);
        (t * PI * 0.5).sin()
    })
}

fn stitch(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let (d, _) = seg_dist(x, y, -0.45, 0.0, 0.45, 0.0);
        let r = 0.13;
        let dome = (1.0 - (d / r).min(1.0).powi(2)).sqrt() * aa(r - d, px);
        // twisted thread plies
        let ply = 0.82 + 0.18 * (0.5 + 0.5 * ((x * 1.0 + y * 1.6) * TAU * 6.0).cos());
        dome * ply
    })
}

/// A raised thread from `a` to `b`: a rounded profile with twisted plies,
/// like `stitch`.
fn thread(x: f32, y: f32, a: (f32, f32), b: (f32, f32), r: f32, px: f32) -> f32 {
    let (d, _) = seg_dist(x, y, a.0, a.1, b.0, b.1);
    let dome = (1.0 - (d / r).min(1.0).powi(2)).sqrt() * aa(r - d, px);
    let ply = 0.82 + 0.18 * (0.5 + 0.5 * ((x * 1.0 + y * 1.6) * TAU * 6.0).cos());
    dome * ply
}

/// Two parallel stitches: a double row of thread along the path.
fn double_stitch(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let a = thread(x, y, (-0.38, -0.24), (0.38, -0.24), 0.1, px);
        let b = thread(x, y, (-0.38, 0.24), (0.38, 0.24), 0.1, px);
        a.max(b)
    })
}

/// An X of two threads, the upper one over the lower.
fn cross_stitch(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let under = thread(x, y, (-0.36, 0.36), (0.36, -0.36), 0.1, px) * 0.85;
        let over = thread(x, y, (-0.36, -0.36), (0.36, 0.36), 0.1, px);
        under.max(over)
    })
}

/// One diagonal thread: in a row along a path it makes a whip stitch.
fn slanted_stitch(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| thread(x, y, (-0.22, -0.4), (0.22, 0.4), 0.12, px))
}

fn ring(size: u32) -> Vec<f32> {
    pixels(size, |x, y, _| {
        let r = (x * x + y * y).sqrt();
        let t = ((r - 0.55) / 0.2).abs();
        (1.0 - t * t).max(0.0).sqrt()
    })
}

fn washer(size: u32) -> Vec<f32> {
    pixels(size, |x, y, px| {
        let r = (x * x + y * y).sqrt();
        let d = (r - 0.3).min(0.78 - r);
        bevel(d, 0.07, px)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphas_render_and_fade_out() {
        assert!(BRUSH_ALPHAS.len() >= 16);
        for size in [32u32, 128] {
            let s = size as usize;
            for b in BRUSH_ALPHAS {
                let img = b.render(size);
                assert_eq!(img.len(), s * s, "{}", b.name);
                assert_eq!(img, b.render(size), "{} not deterministic", b.name);
                for i in 0..s {
                    for v in [img[i], img[(s - 1) * s + i], img[i * s], img[i * s + s - 1]] {
                        assert_eq!(v, 0, "{} touches the border at {size}", b.name);
                    }
                }
                let max = *img.iter().max().unwrap();
                let sum: u64 = img.iter().map(|&v| v as u64).sum();
                // Thin-line tips are rightly faint at thumbnail sizes.
                let min_sum = if size >= 128 { 255 * s as u64 / 4 } else { 255 };
                assert!(max > 96 && sum > min_sum, "{} is (nearly) empty at {size}", b.name);
            }
        }
    }

    #[test]
    #[ignore]
    fn timing_1024() {
        for b in BRUSH_ALPHAS {
            let start = std::time::Instant::now();
            let _ = b.render(1024);
            println!("{:<16} {:>7.1} ms", b.name, start.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Writes a contact sheet of all tips at 192 px and at 48 px.
    #[test]
    #[ignore]
    fn alpha_contact_sheet() {
        let dir = std::env::var("SHEET_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
        for (cell, file) in [(192u32, "brushes_sheet.png"), (48, "brushes_sheet_small.png")] {
            let cols = 7u32;
            let gap = 6;
            let rows = (BRUSH_ALPHAS.len() as u32).div_ceil(cols);
            let mut sheet = image::GrayImage::from_pixel(cols * (cell + gap) + gap, rows * (cell + gap) + gap, image::Luma([0]));
            for (i, b) in BRUSH_ALPHAS.iter().enumerate() {
                let img = b.render(cell);
                let (ox, oy) = (gap + (i as u32 % cols) * (cell + gap), gap + (i as u32 / cols) * (cell + gap));
                for y in 0..cell {
                    for x in 0..cell {
                        let v = img[(y * cell + x) as usize];
                        // faint frame so the cell bounds are visible
                        let frame = x == 0 || y == 0 || x == cell - 1 || y == cell - 1;
                        sheet.put_pixel(ox + x, oy + y, image::Luma([if frame { v.max(50) } else { v }]));
                    }
                }
            }
            sheet.save(dir.join(file)).unwrap();
        }
    }
}
