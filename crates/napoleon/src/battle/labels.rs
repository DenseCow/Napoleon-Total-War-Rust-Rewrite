//! Unit labels: name, men, morale and ammunition above each unit, drawn with the game's own
//! bitmap font (`font\ingame_14.cuf`, read with `ntw_formats::font`), so names such as
//! "18th Regiment d’Infanterie de Ligne “The Brave”" show their typographic quotes.
//!
//! PLACEHOLDER: the original shows unit cards and floating banners, not these text labels (a
//! test-harness readout). Which `.cuf` the original uses for unit names is UNKNOWN; "Ingame 14"
//! is the in-battle family. The 1-pixel black shadow is ours, for readability.
//!
//! A label that would cover the HUD text in the top-left corner is hidden.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::ComputedNode;
use ntw_formats::font::{CufFont, font_file_for};
use ntw_formats::pack::Vfs;

use super::BattleSim;
use super::hud::HudPointer;
use super::view::{UnitLabel, UnitView, world_of};

/// The label font: `None` falls back to Bevy's default font (which lacks ’ “ ”).
#[derive(Resource)]
pub struct LabelFont(pub Option<CufFont>);

/// The font name the labels use (a layout font name, see `font_file_for`).
const LABEL_FONT: &str = "Ingame 14, Normal";

impl LabelFont {
    /// Reads the font from the install.
    pub fn load(vfs: Option<&Vfs>) -> Self {
        let font = font_file_for(LABEL_FONT)
            .and_then(|p| vfs?.read(&p).map_err(|e| warn!("{p}: {e}")).ok())
            .and_then(|b| CufFont::read(&b).map_err(|e| warn!("{LABEL_FONT}: {e:?}")).ok());
        Self(font)
    }
}

/// Draws `lines` (white, with a black shadow) into an RGBA image.
pub fn rasterise(font: &CufFont, lines: &[&str]) -> Image {
    let line_h = font.line_height().max(font.ascent() + 4).max(1);
    let w = (lines.iter().map(|l| font.text_width(l)).max().unwrap_or(1) + 1).max(1) as usize;
    let h = (line_h * lines.len() as i32 + 1).max(1) as usize;
    let mut cov = vec![0u8; w * h];
    for (row, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut pen = 0i32;
        for (i, &ch) in chars.iter().enumerate() {
            if let Some((_, g)) = font.glyph(ch)
                && g.top != i8::MIN
            {
                let bmp = font.bitmap(g);
                let top = row as i32 * line_h + font.ascent() - i32::from(g.top);
                for gy in 0..i32::from(g.height) {
                    for gx in 0..i32::from(g.width) {
                        let (x, y) = (pen + gx, top + gy);
                        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                            let o = y as usize * w + x as usize;
                            cov[o] = cov[o].max(bmp[(gy * i32::from(g.width) + gx) as usize]);
                        }
                    }
                }
            }
            pen += font.advance(ch, chars.get(i + 1).copied());
        }
    }
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let text = cov[y * w + x];
            let shadow = if x > 0 && y > 0 { cov[(y - 1) * w + x - 1] } else { 0 };
            let o = (y * w + x) * 4;
            let a = text.max(shadow);
            // Text over shadow: white where the glyph covers, black where only the shadow does.
            let v = if a == 0 { 0 } else { (u32::from(text) * 255 / u32::from(a)) as u8 };
            px[o..o + 4].copy_from_slice(&[v, v, v, a]);
        }
    }
    let mut img = Image::new(
        Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    img.sampler = ImageSampler::nearest();
    img
}

/// The label lines of a unit.
fn label_lines(sim: &BattleSim, id: u32) -> Option<Vec<String>> {
    let unit = sim.battle.units.iter().find(|u| u.id == id)?;
    let name = sim.info.iter().find(|i| i.id == id).map_or("?", |i| i.name.as_str());
    let mut lines = vec![name.to_owned(), format!("{} men - {:?}", unit.men, unit.morale.state)];
    // Shooting units also show their ammunition and whether they are reloading.
    if unit.missile.is_some() {
        let state = if unit.ammunition == 0 {
            "out of ammo"
        } else if unit.reload_ticks_left > 0 {
            "reloading"
        } else {
            "loaded"
        };
        lines.push(format!("{} rounds per man - {state}", unit.ammunition));
    }
    Some(lines)
}

/// Every frame: place each label over its unit, re-draw its text when it changed, and hide it
/// where it would cover the HUD.
#[allow(clippy::type_complexity)]
pub fn update_labels(
    sim: Res<BattleSim>,
    font: Option<Res<LabelFont>>,
    mut images: ResMut<Assets<Image>>,
    mut labels: Query<(&mut UnitLabel, &mut Node, Option<&mut ImageNode>, Option<&mut Text>, &ComputedNode)>,
    hud: Option<Res<HudPointer>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    views: Query<(&UnitView, &Transform)>,
) {
    let (camera, cam_tf) = *camera;
    // The original HUD's panels (cards, orders, popups); labels are hidden over them, and all of
    // them once the battle is decided (the end-of-battle popups and results screen).
    let panels: &[Rect] = hud.as_ref().map_or(&[], |h| h.panels.as_slice());
    let hide_all = sim.phase == super::BattlePhase::Finished;
    for (mut label, mut node, image, text, computed) in &mut labels {
        let Some(unit) = sim.battle.units.iter().find(|u| u.id == label.id) else { continue };
        let Some(lines) = label_lines(&sim, label.id) else { continue };
        let joined = lines.join("\n");
        if label.text != joined {
            match (font.as_ref().and_then(|f| f.0.as_ref()), image, text) {
                (Some(f), Some(mut image), _) => {
                    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
                    let img = rasterise(f, &refs);
                    node.width = Val::Px(img.width() as f32);
                    node.height = Val::Px(img.height() as f32);
                    // A new label still shows the shared default image: never overwrite that one.
                    if !label.text.is_empty()
                        && let Some(mut old) = images.get_mut(&image.image)
                    {
                        *old = img;
                    } else {
                        image.image = images.add(img);
                    }
                }
                (_, _, Some(mut text)) => text.0.clone_from(&joined),
                _ => {}
            }
            label.text = joined;
        }
        // Over the unit where it is drawn (`view::DrawnPose`), else where the model has it.
        let drawn = views.iter().find(|(v, _)| v.id == label.id).map(|(_, t)| t.translation);
        let anchor = drawn.unwrap_or_else(|| world_of(Vec2::new(unit.position.0, unit.position.1))) + Vec3::Y * 4.0;
        let Ok(p) = camera.world_to_viewport(cam_tf, anchor) else {
            node.display = Display::None;
            continue;
        };
        let size = computed.size() * computed.inverse_scale_factor();
        let top_left = Vec2::new(p.x - size.x * 0.5, p.y - size.y);
        node.left = Val::Px(top_left.x);
        node.top = Val::Px(top_left.y);
        let rect = Rect::from_corners(top_left, top_left + size.max(Vec2::ONE));
        let covers_hud = hide_all || unit.off_field() || panels.iter().any(|h| !h.intersect(rect).is_empty());
        node.display = if covers_hud { Display::None } else { Display::Flex };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped `ingame_14.cuf` has the typographic quotes the unit names use, and the
    /// rasteriser draws them (needs the install; skipped otherwise).
    #[test]
    fn label_font_has_typographic_quotes() {
        let Ok(vfs) = Vfs::open_install(crate::config::game_data_dir()) else { return };
        let Some(font) = LabelFont::load(Some(&vfs)).0 else { return };
        for ch in ['’', '“', '”', 'é'] {
            assert!(font.glyph(ch).is_some(), "{ch} missing");
        }
        let img = rasterise(&font, &["18th Regiment d’Infanterie de Ligne “The Brave”", "160 men"]);
        assert!(img.width() > 100 && img.height() >= 2 * font.line_height() as u32);
        let data = img.data.as_ref().unwrap();
        let max_alpha = data.chunks(4).map(|p| p[3]).max().unwrap();
        let white = data.chunks(4).filter(|p| p[3] > 128 && p[0] > 200).count();
        eprintln!("{}x{} max alpha {max_alpha}, {white} white pixels", img.width(), img.height());
        assert!(max_alpha > 200 && white > 100);
    }
}
