//! Region labels: each region's name painted on the campaign map.
//!
//! The place comes from the map data: `campaign_maps\<map>\regions.esf`'s `theatres_and_region_keys`
//! carries, per theatre, its bounds and the `region_keys` array of (key, Coord2d) pairs — the
//! position each region's name belongs at (CONFIRMED by parsing, §5 of
//! `analysis/campaign/CAMPAIGN_MAP.md`; the reader is `RegionMap::labels`). The text is the
//! region's on-screen name, loc `regions_onscreen_<key>` (CONFIRMED keys, e.g.
//! `regions_onscreen_eur_france` = "France"), drawn with the original's own bitmap font
//! (`ntw_formats::font`, a `.cuf` from the install), as glyph coverage from
//! [`CufFont::coverage`] with our own outline added. All the names share one atlas image and one
//! material; each is its own quad entity.
//!
//! HOW the original draws them is UNKNOWN (no address traced, no Ghidra copy free here). That they
//! are the engine's map text and not HUD components is INFERRED from the data: the positions live
//! in `theatres_and_region_keys` in logic map coordinates, one per region of the theatre.
//! PROVISIONAL: the font (`ingame_12`, the campaign HUD's in-game family), the colour and alpha,
//! and the size rule — the names keep the size of the small HUD text whatever the zoom
//! ([`REFERENCE_DISTANCE`], [`UNITS_PER_PIXEL`]), which is what NTW's map reads like in play.
//! Sea regions carry no name (they have no `regions_onscreen_` text, so they are skipped).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use ntw_campaign::map_display::MapDisplay;
use ntw_formats::font::{CufFont, font_file_for};
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;

use crate::GameMode;
use crate::campaign::camera::CampaignCamera;
use crate::campaign::scene::LINE_LIFT;

/// The `.cuf` the region names use (a layout font name, see `font_file_for`). PROVISIONAL: the
/// original's map-label font is UNKNOWN; `ingame_12` is the campaign HUD's in-game family.
const LABEL_FONT: &str = "Ingame 12, Normal";
/// The name colour. PROVISIONAL: the original's colour is UNKNOWN; a light warm white reads on the
/// map's paper colours.
const LABEL_COLOUR: [f32; 4] = [1.0, 0.98, 0.92, 0.85];
/// The camera distance the name size is measured at (the campaign's start view).
const REFERENCE_DISTANCE: f32 = 60.0;
/// One rasterised name pixel in logic units at [`REFERENCE_DISTANCE`], so a name reads as the
/// HUD's small text there and keeps that size at any zoom. PROVISIONAL (see the module comment).
const UNITS_PER_PIXEL: f32 = 0.15;
/// The atlas is at most this wide (paint pixels); names are packed into rows.
const ATLAS_WIDTH: u32 = 1024;
/// Padding between atlas cells (paint pixels), so linear filtering cannot bleed neighbours in.
const ATLAS_PAD: u32 = 2;

/// One name drawn on the map. The quad is built at its [`REFERENCE_DISTANCE`] size around the
/// label's map position, so [`update`] only has to scale it with the zoom to keep the name the same
/// size on screen.
#[derive(Component, Debug, Clone, Copy)]
pub struct RegionLabel;

/// How much bigger a name is than at [`REFERENCE_DISTANCE`] at `distance`: the quad's scale factor.
/// The view itself grows with the distance, so the name keeps its size on screen.
pub fn label_scale(distance: f32) -> f32 {
    distance / REFERENCE_DISTANCE
}

/// One name to draw: the region's key, the map position it belongs at and its text.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionLabelData {
    /// The region key (`theatres_and_region_keys`, e.g. `eur_bretagne`).
    pub key: String,
    /// The label position in logic map (x, z).
    pub position: (f32, f32),
    /// The text to paint.
    pub text: String,
}

/// The region labels of a campaign map: every land region the map file gives a label position for,
/// with its on-screen name. Sea regions and keys with no `regions_onscreen_` text are left out.
pub fn labels(map: &MapDisplay, loc: &Localisation) -> Vec<RegionLabelData> {
    let mut out = Vec::with_capacity(map.regions.labels.len());
    for (key, position) in &map.regions.labels {
        if map.regions.regions.iter().any(|r| &r.key == key && r.is_sea) {
            continue;
        }
        let Some(text) = loc.get(&format!("regions_onscreen_{key}")) else { continue };
        out.push(RegionLabelData { key: key.clone(), position: *position, text: text.to_owned() });
    }
    out
}

/// Reads the label font from the install (`None` if it is not there).
fn load_font(vfs: &Vfs) -> Option<CufFont> {
    let path = font_file_for(LABEL_FONT)?;
    let bytes = vfs.read(&path).map_err(|e| warn!("{path}: {e}")).ok()?;
    CufFont::read(&bytes).map_err(|e| warn!("{LABEL_FONT}: {e:?}")).ok()
}

/// Where each name goes in the atlas: its cell's left/top and size, in rasterised pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Packs `sizes` (name pixel sizes, in the order they are drawn) into atlas rows of at most
/// [`ATLAS_WIDTH`], and returns each cell and the atlas size. A name wider than the atlas gets its
/// own row (a `.cuf` name never is).
pub fn atlas_layout(sizes: &[(u32, u32)]) -> (Vec<Cell>, u32, u32) {
    let row_h = sizes.iter().map(|s| s.1).max().unwrap_or(0);
    let (mut cells, mut x, mut y, mut w, mut h) = (Vec::with_capacity(sizes.len()), 0u32, 0u32, 0u32, 0u32);
    for &(cw, ch) in sizes {
        if cw == 0 || ch == 0 {
            cells.push(Cell { x: 0, y: 0, w: 0, h: 0 });
            continue;
        }
        if x > 0 && x + cw + ATLAS_PAD > ATLAS_WIDTH {
            x = 0;
            y += row_h + ATLAS_PAD;
        }
        cells.push(Cell { x, y, w: cw, h: ch });
        x += cw + ATLAS_PAD;
        w = w.max(x.saturating_sub(ATLAS_PAD));
        h = y + ch;
    }
    (cells, w.max(1), h.max(1))
}

/// The four corners of a name quad, as offsets in units from its centre, and their texture
/// coordinates in the atlas cell. `units_per_pixel` scales the rasterised pixels; the order is
/// south-west, south-east, north-east, north-west, so a name reads the right way round seen from
/// above (our map: +x east, -z north). `drape` lifts each corner to the ground there, so the name
/// lies on the terrain instead of cutting through it.
fn quad(cell: Cell, atlas: (u32, u32), units_per_pixel: f32, drape: &dyn Fn(f32, f32) -> f32) -> ([Vec3; 4], [Vec2; 4]) {
    let hw = cell.w as f32 * units_per_pixel * 0.5;
    let hh = cell.h as f32 * units_per_pixel * 0.5;
    let pos = [
        Vec3::new(-hw, drape(-hw, hh), hh),
        Vec3::new(hw, drape(hw, hh), hh),
        Vec3::new(hw, drape(hw, -hh), -hh),
        Vec3::new(-hw, drape(-hw, -hh), -hh),
    ];
    let (aw, ah) = (atlas.0.max(1) as f32, atlas.1.max(1) as f32);
    let uv = [
        Vec2::new(cell.x as f32 / aw, (cell.y + cell.h) as f32 / ah),
        Vec2::new((cell.x + cell.w) as f32 / aw, (cell.y + cell.h) as f32 / ah),
        Vec2::new((cell.x + cell.w) as f32 / aw, cell.y as f32 / ah),
        Vec2::new(cell.x as f32 / aw, cell.y as f32 / ah),
    ];
    (pos, uv)
}

/// Draws one name into its atlas cell: the glyphs in white over a one-pixel dark halo, so a name
/// reads on the map's pale land and on its dark forests alike. `glyph` is the name's coverage, one
/// byte per pixel, `w` x `h` ([`CufFont::coverage`]). PROVISIONAL look (the original's
/// region-name colour and outline are UNKNOWN).
fn composite(atlas: &mut [u8], aw: u32, cell: &Cell, glyph: &[u8], w: u32, h: u32) {
    let cov = |x: i32, y: i32| -> u8 {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 { 0 } else { glyph[(y as u32 * w + x as u32) as usize] }
    };
    for y in 0..cell.h {
        for x in 0..cell.w {
            let (px, py) = (x as i32, y as i32);
            let text = cov(px, py);
            let halo = text > 0
                || [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)].iter().any(|(dx, dy)| cov(px + dx, py + dy) >= 128);
            if !halo {
                continue;
            }
            let o = ((cell.y + y) * aw + cell.x + x) as usize * 4;
            // The glyph keeps its own coverage; the halo is a flat dark outline.
            let (v, a) = if text > 0 { (255u8, text) } else { (16u8, 190u8) };
            atlas[o..o + 4].copy_from_slice(&[v, v, v, a]);
        }
    }
}

/// One name's quad mesh: the corners around its own map position, draped on the ground.
fn quad_mesh(map: &MapDisplay, label: &RegionLabelData, cell: Cell, atlas: (u32, u32)) -> Mesh {
    let (x, z) = label.position;
    let centre = map.height_at(x, z).max(0.0) + LINE_LIFT;
    let (pos, uv) = quad(cell, atlas, UNITS_PER_PIXEL, &|dx, dz| map.height_at(x + dx, z + dz).max(0.0) - centre);
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.iter().map(|v| v.to_array()).collect::<Vec<_>>());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.iter().map(|v| v.to_array()).collect::<Vec<_>>());
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

/// `OnEnter(GameMode::Campaign)`: paints every region name into one atlas and lays one quad per
/// name on the map. `NAPOLEON_CAMPAIGN_REGION_LABELS=0` leaves them out (for comparisons).
pub fn spawn(
    commands: &mut Commands,
    vfs: &Vfs,
    map: &MapDisplay,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    if std::env::var("NAPOLEON_CAMPAIGN_REGION_LABELS").is_ok_and(|v| v == "0") {
        return;
    }
    let Some(font) = load_font(vfs) else {
        warn!("Campaign region labels: font {LABEL_FONT} not readable");
        return;
    };
    let loc = Localisation::from_vfs(vfs).unwrap_or_else(|e| {
        warn!("Campaign region labels: localisation not loaded: {e}");
        Localisation::new()
    });
    let names = labels(map, &loc);
    if names.is_empty() {
        warn!("Campaign region labels: none");
        return;
    }

    // One cell per name, all names rasterised into one atlas image.
    let drawn: Vec<(u32, u32, Vec<u8>)> = names.iter().map(|l| font.coverage(&l.text)).collect();
    let sizes: Vec<(u32, u32)> = drawn.iter().map(|(w, h, _)| (*w, *h)).collect();
    let (cells, aw, ah) = atlas_layout(&sizes);
    let mut pixels = vec![0u8; aw as usize * ah as usize * 4];
    for ((w, h, cov), cell) in drawn.iter().zip(&cells) {
        composite(&mut pixels, aw, cell, cov, *w, *h);
    }
    let mut atlas = Image::new(
        Extent3d { width: aw, height: ah, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    atlas.sampler = ImageSampler::linear();
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(atlas)),
        base_color: Color::srgba(LABEL_COLOUR[0], LABEL_COLOUR[1], LABEL_COLOUR[2], LABEL_COLOUR[3]),
        // `AlphaMode::Blend` puts the mesh in Bevy's transparent pass, which leaves the depth write
        // off (bevy_pbr `render/mesh.rs`), so the names do not fight each other.
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    for (label, cell) in names.iter().zip(&cells) {
        let (x, z) = label.position;
        let centre = map.height_at(x, z).max(0.0) + LINE_LIFT;
        commands.spawn((
            Mesh3d(meshes.add(quad_mesh(map, label, *cell, (aw, ah)))),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, centre, -z),
            RegionLabel,
            Name::new(format!("region label {}", label.key)),
            DespawnOnExit(GameMode::Campaign),
        ));
    }
    info!(
        "Campaign: {} region labels ({aw}x{ah} atlas, {:.3} units per name pixel at distance {REFERENCE_DISTANCE})",
        names.len(),
        UNITS_PER_PIXEL
    );
}

/// Keeps every name the same size on screen: the quad is scaled with the camera's distance, so a
/// name reads as small map text at any zoom (PROVISIONAL size rule, see the module comment).
pub fn update(rig: Single<&CampaignCamera>, mut labels: Query<&mut Transform, With<RegionLabel>>) {
    let k = label_scale(rig.distance);
    for mut t in &mut labels {
        t.scale = Vec3::new(k, 1.0, k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The atlas packs names into rows without overlapping, and inside its width.
    #[test]
    fn atlas_rows_do_not_overlap() {
        let sizes = vec![(120u32, 14u32), (60, 14), (200, 14), (30, 14), (90, 14)];
        let (cells, w, h) = atlas_layout(&sizes);
        assert_eq!(cells.len(), sizes.len());
        assert!(w <= ATLAS_WIDTH, "atlas {w} wider than {ATLAS_WIDTH}");
        for (i, c) in cells.iter().enumerate() {
            assert_eq!((c.w, c.h), sizes[i]);
            assert!(c.x + c.w <= w && c.y + c.h <= h, "cell {i} {c:?} outside {w}x{h}");
            for (j, o) in cells.iter().enumerate().take(i) {
                let apart = c.x + c.w + ATLAS_PAD <= o.x || o.x + o.w + ATLAS_PAD <= c.x || c.y + c.h + ATLAS_PAD <= o.y || o.y + o.h + ATLAS_PAD <= c.y;
                assert!(apart, "cells {i} {c:?} and {j} {o:?} overlap");
            }
        }
    }

    /// A long name gets its own row instead of running off the atlas.
    #[test]
    fn atlas_wraps_a_wide_name() {
        let (cells, w, h) = atlas_layout(&[(ATLAS_WIDTH - 2, 10), (ATLAS_WIDTH - 2, 10), (40, 10)]);
        assert_eq!((w, h), (ATLAS_WIDTH - 2, 10 * 3 + ATLAS_PAD * 2));
        assert_eq!(cells[2].y, 2 * (10 + ATLAS_PAD));
    }

    /// A name is centred on its map position, north up, and its texture coordinates cover the cell.
    #[test]
    fn quad_is_centred_and_reads_north_up() {
        let cell = Cell { x: 10, y: 20, w: 100, h: 12 };
        let flat = |_: f32, _: f32| 0.0;
        let (pos, uv) = quad(cell, (256, 64), UNITS_PER_PIXEL, &flat);
        let w = 100.0 * UNITS_PER_PIXEL;
        let h = 12.0 * UNITS_PER_PIXEL;
        // South-west first: +x east, -z north, centred on the origin.
        assert!((pos[0].x + w / 2.0).abs() < 1e-5 && (pos[0].z - h / 2.0).abs() < 1e-5);
        assert!((pos[3].z + h / 2.0).abs() < 1e-5);
        // v grows northwards, u eastwards, over the cell only.
        assert!(uv[0].y > uv[3].y && uv[3].x < uv[1].x);
        for t in uv {
            assert!((t.x * 256.0 - 10.0).abs() < 0.01 || (t.x * 256.0 - 110.0).abs() < 0.01);
            assert!((t.y * 64.0 - 20.0).abs() < 0.01 || (t.y * 64.0 - 32.0).abs() < 0.01);
        }
        // The drape lifts the corners it is given.
        let draped = quad(cell, (256, 64), UNITS_PER_PIXEL, &|_, _| 1.5);
        assert!(draped.0.iter().all(|v| (v.y - 1.5).abs() < 1e-6));
    }

    /// Every name is the same size on screen whatever the zoom, and keeps its cell's aspect.
    #[test]
    fn zoom_scaling_keeps_the_screen_size() {
        let cell = Cell { x: 0, y: 0, w: 80, h: 14 };
        let flat = |_: f32, _: f32| 0.0;
        let (pos, _) = quad(cell, (1024, 94), UNITS_PER_PIXEL, &flat);
        let width_at_start = pos[1].x - pos[0].x;
        assert!((width_at_start / (pos[0].z - pos[3].z) - 80.0 / 14.0).abs() < 1e-4, "the cell's aspect");
        // The quad grows exactly as fast as the view, so a name covers the same share of the screen
        // at every zoom (the view's height grows with the camera distance).
        for d in [crate::campaign::camera::MIN_DISTANCE, 35.0, 100.0, crate::campaign::camera::MAX_DISTANCE] {
            let view = d / REFERENCE_DISTANCE;
            assert!(((width_at_start * label_scale(d)) / width_at_start - view).abs() < 1e-5);
        }
        assert!(label_scale(crate::campaign::camera::MIN_DISTANCE) < 1.0);
        assert!(label_scale(crate::campaign::camera::MAX_DISTANCE) > 1.0);
    }

    /// A name is drawn white with a dark one-pixel halo, so it reads on any map colour.
    #[test]
    fn the_outline_surrounds_the_glyphs() {
        // One opaque pixel in the middle of a 5x5 name.
        let mut glyph = vec![0u8; 5 * 5];
        glyph[2 * 5 + 2] = 200;
        let cell = Cell { x: 0, y: 0, w: 5, h: 5 };
        let mut atlas = vec![0u8; 5 * 5 * 4];
        composite(&mut atlas, 5, &cell, &glyph, 5, 5);
        let px = |x: usize, y: usize| {
            let o = (y * 5 + x) * 4;
            (atlas[o], atlas[o + 3])
        };
        assert_eq!(px(2, 2), (255, 200), "the glyph keeps its coverage, in white");
        for (x, y) in [(2, 1), (2, 3), (1, 2), (3, 2), (1, 1), (3, 3)] {
            let (v, a) = px(x, y);
            assert!(v < 32 && a > 128, "the halo pixel ({x}, {y}) is dark: {v} {a}");
        }
        assert_eq!(px(0, 0), (0, 0), "a pixel two steps away stays clear");
    }

    /// The real map: every playable region has a label position and a loc name (needs the install).
    #[test]
    fn every_land_region_has_a_label_and_a_name() {
        let Ok(vfs) = Vfs::open_install(crate::config::game_data_dir()) else { return };
        let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
        let Ok(map) = ntw_formats::campaign_map::CampaignMap::load(&files, "nap_europe") else { return };
        let map = ntw_campaign::source::original_display(&files, map);
        let Ok(loc) = Localisation::from_vfs(&vfs) else { return };
        let names = labels(&map, &loc);
        eprintln!("{} label positions, {} names", map.regions.labels.len(), names.len());
        // `theatres_and_region_keys` holds one label per land region with a settlement: 72 on
        // Europe. 101 is every region of regions.esf, sea and settlement-less filler included
        // (eur_map_west/east, all, eur_lakes, eur_tyrolland); those have no label record.
        let settled = map.regions.regions.iter().filter(|r| !r.is_sea && r.settlement.is_some()).count();
        assert_eq!((map.regions.labels.len(), settled), (72, 72), "the European map's theatre region list");
        // No sea region is named, and every name is a region of the map inside its bounds.
        for l in &names {
            let r = map.regions.regions.iter().find(|r| r.key == l.key).expect("label of a real region");
            assert!(!r.is_sea, "{} is a sea region", l.key);
            assert!(l.text.chars().any(|c| c.is_ascii_alphabetic()), "{} has no name text", l.key);
            assert!(l.position.0 > map.regions.bounds_min.0 && l.position.0 < map.regions.bounds_max.0);
            assert!(l.position.1 > map.regions.bounds_min.1 && l.position.1 < map.regions.bounds_max.1);
        }
        // The names rasterise into the atlas, and each quad sits on its region's ground.
        let Some(font) = load_font(&vfs) else { return };
        let drawn: Vec<(u32, u32, Vec<u8>)> = names.iter().map(|l| font.coverage(&l.text)).collect();
        let sizes: Vec<(u32, u32)> = drawn.iter().map(|(w, h, _)| (*w, *h)).collect();
        let (cells, w, h) = atlas_layout(&sizes);
        assert_eq!(cells.len(), names.len());
        assert!(w <= ATLAS_WIDTH && w > 0 && h > 0);
        for ((label, cell), (gw, gh, cov)) in names.iter().zip(&cells).zip(&drawn) {
            assert_eq!((cell.w, cell.h), (*gw, *gh));
            assert!(cov.iter().any(|&c| c > 0), "{}: the name drew no pixels", label.key);
            let mesh = quad_mesh(&map, label, *cell, (w, h));
            let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().expect("float3 corners");
            assert_eq!(positions.len(), 4, "{}: a name is one quad", label.key);
            for v in positions {
                assert!(v.iter().all(|c| c.is_finite()), "{}: corner {v:?} is not a number", label.key);
            }
        }
    }
}
