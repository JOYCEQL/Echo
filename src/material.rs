//! Static, device-pixel grain in Rune's painter stream; no bitmap or idle animation.
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::KIND_EFFECT;
use rune_fx::{Fx, FxFrame, FxOptions, FxRenderer, FxSlot, GrainParams, Shape};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub transparency: u8,
    pub grain: u8,
    #[serde(default = "reference_colors")]
    pub colors: Vec<Tint>,
    #[serde(default)]
    pub mode: LookMode,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LookMode {
    Auto,
    #[default]
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "StoredTint")]
pub struct Tint {
    pub hue: u16,
    pub saturation: u8,
    pub lightness: u8,
}
#[derive(serde::Deserialize)]
struct StoredTint {
    hue: u16,
    saturation: u8,
    lightness: Option<u8>,
}
impl TryFrom<StoredTint> for Tint {
    type Error = &'static str;
    fn try_from(old: StoredTint) -> Result<Self, Self::Error> {
        if old.hue > 360 || old.saturation > 100 || old.lightness.is_some_and(|l| l > 100) {
            return Err("invalid saved Look color");
        }
        if let Some(lightness) = old.lightness {
            return Ok(Self {
                hue: old.hue,
                saturation: old.saturation,
                lightness,
            });
        }
        // Explicit migration of the previous pastel HSV field into OKLCH.
        let h = old.hue as f32 / 60.;
        let chroma = old.saturation as f32 / 100. * 0.28;
        let x = chroma * (1. - (h % 2. - 1.).abs());
        let rgb = match h as u32 {
            0 => [chroma, x, 0.],
            1 => [x, chroma, 0.],
            2 => [0., chroma, x],
            3 => [0., x, chroma],
            4 => [x, 0., chroma],
            _ => [chroma, 0., x],
        };
        Ok(Self::from_linear(
            rgb.map(|c| rune::color::srgb_decode(c + 0.96 - chroma)),
        ))
    }
}
impl Default for Tint {
    fn default() -> Self {
        Self {
            hue: 210,
            saturation: 0,
            lightness: 96,
        }
    }
}
impl Tint {
    pub fn from_hex(rgb: u32) -> Self {
        let c = crate::ui::c(rgb);
        Self::from_linear([c.r, c.g, c.b])
    }
    pub fn from_linear([r, g, b]: [f32; 3]) -> Self {
        let l = (0.41222147 * r + 0.53633255 * g + 0.051445995 * b).cbrt();
        let m = (0.2119035 * r + 0.6806995 * g + 0.10739696 * b).cbrt();
        let s = (0.08830246 * r + 0.28171885 * g + 0.6299787 * b).cbrt();
        let light = 0.21045426 * l + 0.7936178 * m - 0.004072047 * s;
        let a = 1.9779985 * l - 2.4285922 * m + 0.4505937 * s;
        let b = 0.025904037 * l + 0.78277177 * m - 0.80867577 * s;
        let h = b.atan2(a).rem_euclid(std::f32::consts::TAU);
        let c = a.hypot(b);
        Self {
            hue: h.to_degrees().round() as u16 % 360,
            saturation: (c / Self::gamut(light, h).max(0.00001) * 100.)
                .round()
                .clamp(0., 100.) as u8,
            lightness: (light * 100.).round().clamp(0., 100.) as u8,
        }
    }
    pub fn linear(l: f32, c: f32, h: f32) -> [f32; 3] {
        let a = c * h.cos();
        let b = c * h.sin();
        let ll = (l + 0.39633778 * a + 0.21580376 * b).powi(3);
        let m = (l - 0.105561346 * a - 0.06385417 * b).powi(3);
        let s = (l - 0.08948418 * a - 1.2914855 * b).powi(3);
        [
            4.0767417 * ll - 3.3077116 * m + 0.23096994 * s,
            -1.268438 * ll + 2.6097574 * m - 0.3413194 * s,
            -0.0041960863 * ll - 0.7034186 * m + 1.7076147 * s,
        ]
    }
    pub fn gamut(l: f32, h: f32) -> f32 {
        let (mut lo, mut hi) = (0., 0.37);
        for _ in 0..14 {
            let mid = (lo + hi) / 2.;
            if Self::linear(l, mid, h).iter().all(|c| *c >= 0. && *c <= 1.) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
    pub fn color(self) -> rune::Color {
        let l = self.lightness as f32 / 100.;
        let h = (self.hue as f32).to_radians();
        let [r, g, b] = Self::linear(l, Self::gamut(l, h) * self.saturation as f32 / 100., h);
        rune::Color {
            r: r.clamp(0., 1.),
            g: g.clamp(0., 1.),
            b: b.clamp(0., 1.),
            a: 1.,
        }
    }
    pub fn point(self) -> [f32; 2] {
        let h = (self.hue as f32).to_radians();
        let r = self.saturation as f32 / 100. * 0.42;
        [0.5 + h.cos() * r, 0.5 + h.sin() * r]
    }
    pub fn at(mut self, x: f32, y: f32) -> Self {
        let (x, y) = (x - 0.5, y - 0.5);
        let distance = x.hypot(y);
        self.saturation = (distance / 0.42 * 100.).round().clamp(0., 100.) as u8;
        if distance > 0.0001 {
            self.hue = y.atan2(x).to_degrees().rem_euclid(360.).round() as u16 % 360;
        }
        self
    }
}
impl Settings {
    pub fn is_dawn(&self) -> bool { self.colors.as_slice() == DAWN_TINTS.as_slice() }
    pub fn is_porcelain(&self) -> bool { self.colors.as_slice() == PORCELAIN_TINTS.as_slice() }
    pub fn is_warm_white(&self) -> bool { self.colors.as_slice() == WARM_WHITE_TINTS.as_slice() }
    pub fn is_minimal_white(&self) -> bool { self.is_porcelain() || self.is_warm_white() }
    pub fn opacity(&self) -> f32 {
        1. - 0.84 * self.transparency as f32 / 100.
    }
    #[cfg(target_os = "macos")]
    pub fn native_backdrop(&self, system_dark: bool) -> rune::SystemMaterial {
        let colors: Vec<_> = (0..self.colors.len()).map(|i| self.color(i, system_dark)).collect();
        let dark = self.mode == LookMode::Dark || (self.mode == LookMode::Auto && system_dark);
        let coating_opacity = 1. - f32::from(self.transparency.min(100)) / 100.;
        rune::SystemMaterial::WindowBackdrop(rune::WindowBackdrop::sidebar(&colors, coating_opacity, dark))
    }
    /// One material palette for the window, Look swatches and editor previews.
    /// One color stays solid; multiple colors span one continuous window coating.
    pub fn surface_stops(&self, system_dark: bool) -> Vec<(f32, rune::Color)> {
        let alpha = self.opacity();
        if self.colors.len() == 1 {
            let base = self.color(0, system_dark).alpha(alpha);
            return vec![(0., base), (1., base)];
        }
        (0..self.colors.len()).map(|i| (
            i as f32 / (self.colors.len() - 1) as f32,
            self.color(i, system_dark).alpha(alpha),
        )).collect()
    }
    fn palette_color_weight(&self) -> f32 {
        let chroma = self.colors.iter().map(|tint| {
            Tint::gamut(tint.lightness as f32 / 100., (tint.hue as f32).to_radians())
                * tint.saturation as f32 / 100.
        }).fold(0.0_f32, f32::max);
        // Smoothly leave neutral gray rather than jumping at chroma == 0.02.
        let t = ((chroma - 0.01) / 0.03).clamp(0., 1.);
        t * t * (3. - 2. * t)
    }
    pub fn color(&self, index: usize, system_dark: bool) -> rune::Color {
        let tint = self.colors[index];
        if self.mode == LookMode::Dark || (self.mode == LookMode::Auto && system_dark) {
            // YabDesign's NSColor.deep: preserve absolute OKLCH chroma, not
            // its proportion of the smaller dark gamut (which over-saturates pastels).
            let l = tint.lightness as f32 / 100.;
            let h = (tint.hue as f32).to_radians();
            let chroma = Tint::gamut(l, h) * tint.saturation as f32 / 100.;
            // A gradient is one window material. Classifying each stop as gray
            // or colorful separately turns equally pale colors into different
            // dark lightness bands. Resolve one continuous tone for the palette.
            let weight = self.palette_color_weight();
            let neutral = (l * 0.3).clamp(0.2, 0.24);
            let colorful = (l * 0.45).clamp(0.2, 0.35);
            let deep = neutral + (colorful - neutral) * weight;
            let [r, g, b] = Tint::linear(deep, (chroma * 0.6).min(Tint::gamut(deep, h)), h);
            return rune::Color { r: r.clamp(0., 1.), g: g.clamp(0., 1.), b: b.clamp(0., 1.), a: 1. };
        }
        tint.color()
    }
}
/// The reference sidebar's very pale cool-to-warm wash, shared by both modes.
pub const REFERENCE_COLORS: [u32; 3] = [0xedf2fc, 0xf7f7fa, 0xfbf3ee];
pub const PORCELAIN_COLORS: [u32; 1] = [0xf6f7f8];
static PORCELAIN_TINTS: std::sync::LazyLock<[Tint; 1]> =
    std::sync::LazyLock::new(|| PORCELAIN_COLORS.map(Tint::from_hex));
pub const WARM_WHITE_COLORS: [u32; 1] = [0xf4f3ef];
static WARM_WHITE_TINTS: std::sync::LazyLock<[Tint; 1]> =
    std::sync::LazyLock::new(|| WARM_WHITE_COLORS.map(Tint::from_hex));
pub const DAWN_COLORS: [u32; 2] = [0xd6d7ee, 0xf3e1eb];
static DAWN_TINTS: std::sync::LazyLock<[Tint; 2]> =
    std::sync::LazyLock::new(|| DAWN_COLORS.map(Tint::from_hex));
pub const SAND_COLORS: [u32; 2] = [0xece7df, 0xf3ede7];
pub const PAPER_COLORS: [u32; 2] = [0xf4f5f5, 0xfbfaf8];
/// Match a unit-space corner gradient: t = (x / width + y / height) / 2.
/// Rune projects in physical coordinates, so a fixed 135 degrees only matches
/// a square. Recompute for the whole window (never for the navigation width).
pub fn surface_angle(width: f32, height: f32) -> f32 {
    height.atan2(-width)
}
pub fn reference_colors() -> Vec<Tint> {
    REFERENCE_COLORS.into_iter().map(Tint::from_hex).collect()
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            transparency: 0,
            grain: 0,
            colors: PAPER_COLORS.map(Tint::from_hex).to_vec(),
            mode: LookMode::Light,
        }
    }
}

pub const GRAIN: u32 = 0x4543484f;

#[derive(Default)]
pub struct MaterialCompositor {
    renderer: Option<(DeviceKey, FxRenderer)>,
}

impl Compositor<crate::model::State> for MaterialCompositor {
    fn render(&mut self, f: &mut Frame<'_, crate::model::State>) {
        let slots: Vec<_> =
            f.rt.instances
                .iter()
                .enumerate()
                .filter(|(n, i)| i.params[1] == KIND_EFFECT && f.rt.marks.get(*n) == Some(&GRAIN))
                .map(|(n, i)| (n as u32, *i))
                .collect();
        if slots.is_empty() {
            self.renderer = None;
            f.draw_ui();
            return;
        }
        let key = f.device_key();
        if self.renderer.as_ref().is_none_or(|(old, _)| *old != key) {
            self.renderer = Some((
                key,
                FxRenderer::new(f.device, f.format, FxOptions::default()),
            ));
        }
        let (_, fx) = self
            .renderer
            .as_mut()
            .expect("material renderer initialized for this device");
        fx.set_srgb_blend(f.srgb_blend());
        fx.add_format(f.device, f.format);
        fx.add_format(f.device, wgpu::TextureFormat::Rgba16Float);
        let layers: Vec<_> = slots
            .iter()
            .map(|(_, i)| {
                let scale = i.params[3];
                let [x, y, w, h] = i.rect.map(|v| v / scale);
                let radius = i.radii.iter().fold(0.0_f32, |r, v| r.max(v.abs())) / scale;
                Fx::grain(GrainParams {
                    // 100% is the previously calibrated strong grain; the default is softer.
                    amount: 0.18 * f.rt.state.sidebar_material.grain as f32 / 100.
                        * f.rt.state.sidebar_material.opacity(),
                    fps: 0.0,
                    feather: 0.0,
                })
                .on(Shape::rounded(x, y, w, h, radius))
                .seed(42)
                .at(0.0)
            })
            .collect();
        let placements: Vec<_> = slots
            .iter()
            .map(|(_, i)| FxSlot::default().scale(i.params[3]).clip(i.clip, i.fade))
            .collect();
        fx.prepare_with_slots(
            f.device,
            f.queue,
            &FxFrame {
                viewport: [f.size.0, f.size.1],
                scale: 1.0,
                layers: &layers,
                backdrop: None,
                sources: &[],
            },
            &placements,
        );
        let clear = f.rt.clear_color();
        f.ui.prepare(
            f.device,
            f.queue,
            &mut f.rt.text.atlas,
            &f.rt.instances,
            f.size,
        );
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark == GRAIN {
                let idx = slots
                    .iter()
                    .position(|(n, _)| *n == site.index)
                    .expect("grain slot was prepared");
                fx.draw_layer_in(pass, site.format, site.origin, idx);
            }
        });
    }
}
