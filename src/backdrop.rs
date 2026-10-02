use std::{path::Path, sync::Arc};

use image::{
    Rgb, RgbImage,
    imageops::{self, FilterType},
};

use crate::{config, theme};

pub fn bayer(x: usize, y: usize) -> f64 {
    let mut value = 0;
    for bit in 0..3 {
        value = 4 * value + [0, 2, 3, 1][((y >> bit) & 1) * 2 + ((x >> bit) & 1)];
    }
    (value as f64 + 0.5) / 64.0
}

fn presence(y: u32, height: u32) -> f64 {
    let t = ((y as f64 / height.max(1) as f64 - 0.2) / 0.8).clamp(0.0, 1.0);
    1.0 - 0.55 * t * t * (3.0 - 2.0 * t)
}

fn lum(c: theme::Rgb) -> f64 {
    (0.2126 * c.0 as f64 + 0.7152 * c.1 as f64 + 0.0722 * c.2 as f64) / 255.0
}

/// Fórmula de BetterThanEminus/src/bg.js, incluido el redondeo del canvas.
fn sample(
    color: theme::Rgb,
    base: theme::Rgb,
    x: u32,
    y: u32,
    height: u32,
    strength: f64,
    plain: bool,
) -> (theme::Rgb, theme::Rgb, bool) {
    let v = presence(y, height);
    let contrast = ((lum(color) - lum(base)).abs() * 1.6 + 0.1).min(1.0);
    let lit = contrast * v * 1.1 > bayer(x as usize, y as usize);
    let blend = |k: f64| {
        let f = |a: u8, b: u8| {
            (a as f64 + (b as f64 - a as f64) * k * strength)
                .round_ties_even()
                .clamp(0.0, 255.0) as u8
        };
        (f(base.0, color.0), f(base.1, color.1), f(base.2, color.2))
    };
    (
        blend(if plain {
            v
        } else if lit {
            0.35 + 0.65 * v
        } else {
            0.25 * v.powi(3)
        }),
        blend(0.25 * v.powi(3)),
        lit,
    )
}

#[derive(Default)]
pub struct Backdrop {
    pub image: Option<Arc<RgbImage>>,
    pub tone: Option<(f32, f32)>,
}

impl Backdrop {
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let mut reader = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16000);
        limits.max_image_height = Some(16000);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|e| e.to_string())?
            .thumbnail(1920, 1920)
            .to_rgb8();
        self.tone = dominant_tone(&image);
        self.image = Some(Arc::new(image));
        Ok(())
    }

    pub fn import(&mut self, source: &Path) -> Result<String, String> {
        // Valida antes de copiar y de cambiar las preferencias.
        let mut loaded = Self::default();
        loaded.load(source)?;
        let folder = config::directory().join("backgrounds");
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let name = source.file_name().ok_or("La imagen no tiene nombre")?;
        let mut target = folder.join(name);
        let original = std::fs::canonicalize(source).map_err(|e| e.to_string())?;
        if std::fs::canonicalize(&target).ok().as_ref() != Some(&original) {
            if target.exists() && std::fs::read(&target).ok() != std::fs::read(source).ok() {
                target = folder.join(format!("{}-{}", std::process::id(), name.to_string_lossy()));
            }
            config::atomic_write(&target, &std::fs::read(source).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
        *self = loaded;
        Ok(target.to_string_lossy().into_owned())
    }

    pub fn render_pixels(
        &self,
        width: u32,
        height: u32,
        dot: u32,
        base: theme::Rgb,
        intensity: f64,
        plain: bool,
    ) -> RgbImage {
        let dot = dot.max(1);
        let image = self.image.as_ref().unwrap();
        let small = render_cells(image, width, height, dot, base, intensity, plain);
        let (w, h) = small.dimensions();
        imageops::crop_imm(
            &imageops::resize(&small, w * dot, h * dot, FilterType::Nearest),
            0,
            0,
            width,
            height,
        )
        .to_image()
    }
}

fn cover(
    image: &RgbImage,
    width: u32,
    height: u32,
    physical: (u32, u32),
    base: theme::Rgb,
) -> RgbImage {
    let scale = (physical.0 as f64 / image.width() as f64)
        .max(physical.1 as f64 * 0.95 / image.height() as f64);
    let w = (image.width() as f64 * scale * width as f64 / physical.0.max(1) as f64)
        .round()
        .max(1.0) as u32;
    let h = (image.height() as f64 * scale * height as f64 / physical.1.max(1) as f64)
        .round()
        .max(1.0) as u32;
    let scaled = imageops::resize(image, w, h, FilterType::Lanczos3);
    let mut canvas = RgbImage::from_pixel(width, height, Rgb([base.0, base.1, base.2]));
    imageops::overlay(
        &mut canvas,
        &scaled,
        (width as i64 - w as i64) / 2,
        (height as f64 * 0.95 / 2.0 - h as f64 / 2.0).round() as i64,
    );
    canvas
}

/// Una celda del tramado por píxel: la GPU la amplía sin interpolar, así la
/// textura es `dot²` veces más chica que la ventana.
pub fn render_cells(
    image: &RgbImage,
    width: u32,
    height: u32,
    dot: u32,
    base: theme::Rgb,
    intensity: f64,
    plain: bool,
) -> RgbImage {
    let dot = dot.max(1);
    let (w, h) = (width.div_ceil(dot), height.div_ceil(dot));
    let photo = cover(image, w, h, (width, height), base);
    RgbImage::from_fn(w, h, |x, y| {
        let p = photo.get_pixel(x, y).0;
        let c = sample((p[0], p[1], p[2]), base, x, y, h, intensity, plain).0;
        Rgb([c.0, c.1, c.2])
    })
}

fn dominant_tone(image: &RgbImage) -> Option<(f32, f32)> {
    let image = imageops::resize(image, 64, 64, FilterType::Triangle);
    let mut bins = [[0.0f64; 4]; 36];
    for p in image.pixels() {
        let [r, g, b] = p.0.map(|v| v as f64 / 255.0);
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        let l = (hi + lo) / 2.0;
        let d = hi - lo;
        if d < 0.03 || !(0.06..=0.96).contains(&l) {
            continue;
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = (if hi == r {
            (g - b) / d
        } else if hi == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0)
            .rem_euclid(360.0);
        let weight = s.powf(1.5);
        let bin = &mut bins[(h / 10.0) as usize % 36];
        bin[0] += weight;
        bin[1] += s * weight;
        bin[2] += h.to_radians().cos() * weight;
        bin[3] += h.to_radians().sin() * weight;
    }
    let i = (0..36).max_by(|&a, &b| {
        let w = |i: usize| bins[(i + 35) % 36][0] * 0.5 + bins[i][0] + bins[(i + 1) % 36][0] * 0.5;
        w(a).total_cmp(&w(b))
    })?;
    let best = bins[i];
    if best[0] < 0.002 * 4096.0 {
        return None;
    }
    Some((
        best[3].atan2(best[2]).to_degrees().rem_euclid(360.0) as f32,
        (best[1] / best[0]) as f32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_better_than_eminus_javascript() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/tramado_reference.json")).unwrap();
        let width = fixture["width"].as_u64().unwrap() as u32;
        let height = fixture["height"].as_u64().unwrap() as u32;
        let input = fixture["input"].as_array().unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let base = case["base"].as_array().unwrap();
            let base = (
                base[0].as_u64().unwrap() as u8,
                base[1].as_u64().unwrap() as u8,
                base[2].as_u64().unwrap() as u8,
            );
            let expected = case["expected"].as_array().unwrap();
            for y in 0..height {
                for x in 0..width {
                    let i = ((y * width + x) * 4) as usize;
                    let color = (
                        input[i].as_u64().unwrap() as u8,
                        input[i + 1].as_u64().unwrap() as u8,
                        input[i + 2].as_u64().unwrap() as u8,
                    );
                    let actual = sample(
                        color,
                        base,
                        x,
                        y,
                        height,
                        1.0,
                        case["plain"].as_bool().unwrap(),
                    )
                    .0;
                    assert_eq!(
                        [actual.0, actual.1, actual.2],
                        std::array::from_fn::<_, 3, _>(|c| expected[i + c].as_u64().unwrap() as u8),
                        "píxel {x},{y}"
                    );
                }
            }
        }
    }
    #[test]
    fn bayer_and_photo_pixels() {
        assert_eq!(bayer(0, 0), 0.5 / 64.0);
        assert_eq!(bayer(1, 0), 32.5 / 64.0);
        let mut values: Vec<_> = (0..8)
            .flat_map(|y| (0..8).map(move |x| bayer(x, y)))
            .collect();
        values.sort_by(f64::total_cmp);
        assert_eq!(
            values,
            (0..64).map(|i| (i as f64 + 0.5) / 64.0).collect::<Vec<_>>()
        );
        let bg = Backdrop {
            image: Some(RgbImage::from_pixel(64, 64, Rgb([230, 40, 40])).into()),
            ..Default::default()
        };
        let photo = bg.image.as_ref().unwrap();
        let image = render_cells(photo, 40, 20, 2, theme::hex(0x16131f), 0.7, false);
        assert_eq!(image.dimensions(), (20, 10));
        let pixels = bg.render_pixels(40, 20, 2, theme::hex(0x16131f), 0.7, false);
        assert_eq!(pixels.dimensions(), (40, 20));
        assert_eq!(pixels.get_pixel(2, 2), pixels.get_pixel(3, 3));
        assert_eq!(pixels.get_pixel(2, 2), image.get_pixel(1, 1));
        let plain = render_cells(photo, 40, 20, 2, theme::hex(0x16131f), 0.7, true);
        assert_ne!(image, plain);
        assert!(dominant_tone(bg.image.as_ref().unwrap()).unwrap().1 > 0.7);
        assert!(dominant_tone(&RgbImage::from_pixel(8, 8, Rgb([128, 128, 128]))).is_none());
    }
}
