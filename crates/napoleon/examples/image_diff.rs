//! Comparison harness, picture side (`docs/COMPARE_WITH_ORIGINAL.md`): compares one of our
//! screenshots with a screenshot of the original game taken from the same view.
//!
//! ```text
//! cargo run -p napoleon --release --example image_diff -- <ours.png> <original.jpg|png|tga> [diff.png]
//! ```
//! Ours is resampled to the original's size. Prints the mean absolute difference per channel
//! (0..255), the PSNR, the share of pixels whose brightness differs by more than 32, and the
//! correlation of the 16 × 12 block brightness grids (layout: 1.0 = the same light and dark areas in
//! the same places, even when colours differ). With a third path it writes a heat map: black = same,
//! red = different.

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// A decoded picture as RGBA8.
struct Picture {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

fn load(path: &str) -> Result<Picture, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let ext = path.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
    let img = Image::from_buffer(&bytes, ImageType::Extension(&ext), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default())
        .map_err(|e| format!("{path}: {e}"))?;
    let rgba = img.try_into_dynamic().map_err(|e| format!("{path}: {e:?}"))?.to_rgba8();
    Ok(Picture { w: rgba.width(), h: rgba.height(), px: rgba.into_raw() })
}

impl Picture {
    /// Bilinear sample at (u, v) in 0..1.
    fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        let x = (u * self.w as f32 - 0.5).clamp(0.0, (self.w - 1) as f32);
        let y = (v * self.h as f32 - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |x: u32, y: u32, c: usize| f32::from(self.px[((y * self.w + x) * 4) as usize + c]);
        let mut out = [0.0; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let top = p(x0, y0, c) * (1.0 - fx) + p(x1, y0, c) * fx;
            let bottom = p(x0, y1, c) * (1.0 - fx) + p(x1, y1, c) * fx;
            *o = top * (1.0 - fy) + bottom * fy;
        }
        out
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
        eprintln!("usage: image_diff <ours> <original> [diff.png]");
        std::process::exit(2);
    };
    let (ours, orig) = match (load(a), load(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let (w, h) = (orig.w, orig.h);
    let mut sum = [0f64; 3];
    let mut sq = 0f64;
    let mut big = 0usize;
    let (gw, gh) = (16usize, 12usize);
    let mut grid_a = vec![0f64; gw * gh];
    let mut grid_b = vec![0f64; gw * gh];
    let mut heat = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let pa = ours.sample(u, v);
            let i = ((y * w + x) * 4) as usize;
            let pb = [f32::from(orig.px[i]), f32::from(orig.px[i + 1]), f32::from(orig.px[i + 2])];
            for c in 0..3 {
                let d = f64::from((pa[c] - pb[c]).abs());
                sum[c] += d;
                sq += d * d;
            }
            let dl = (luma(pa) - luma(pb)).abs();
            if dl > 32.0 {
                big += 1;
            }
            let g = (y as usize * gh / h as usize) * gw + x as usize * gw / w as usize;
            grid_a[g] += f64::from(luma(pa));
            grid_b[g] += f64::from(luma(pb));
            heat[i] = (dl * 3.0).min(255.0) as u8;
            heat[i + 3] = 255;
        }
    }
    let n = f64::from(w) * f64::from(h);
    let mse = sq / (n * 3.0);
    let psnr = if mse > 0.0 { 10.0 * (255.0f64 * 255.0 / mse).log10() } else { f64::INFINITY };
    let corr = {
        let (ma, mb) = (grid_a.iter().sum::<f64>() / grid_a.len() as f64, grid_b.iter().sum::<f64>() / grid_b.len() as f64);
        let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
        for (x, y) in grid_a.iter().zip(&grid_b) {
            num += (x - ma) * (y - mb);
            da += (x - ma) * (x - ma);
            db += (y - mb) * (y - mb);
        }
        if da > 0.0 && db > 0.0 { num / (da * db).sqrt() } else { 0.0 }
    };
    println!("size: ours {}x{} -> original {w}x{h}", ours.w, ours.h);
    println!("mean abs difference R G B: {:.1} {:.1} {:.1} (0..255)", sum[0] / n, sum[1] / n, sum[2] / n);
    println!("PSNR: {psnr:.2} dB");
    println!("pixels with a brightness difference > 32: {:.1} %", 100.0 * big as f64 / n);
    println!("layout correlation (16x12 brightness blocks): {corr:.3}");
    if let Some(out) = args.get(2) {
        let img = Image::new(
            Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            TextureDimension::D2,
            heat,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        match img.try_into_dynamic() {
            Ok(d) => match d.save(out) {
                Ok(()) => println!("heat map: {out}"),
                Err(e) => eprintln!("{out}: {e}"),
            },
            Err(e) => eprintln!("heat map: {e:?}"),
        }
    }
}
