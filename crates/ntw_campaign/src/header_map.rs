//! The save header's territory pictures (`SAVE_GAME_HEADER/MAPS`), which the front end's Load Game
//! page shows (SAVE_COMPAT.md §25).
//!
//! The rule, CONFIRMED pixel for pixel on the original's own saves (`header_map_probe`: every pixel
//! of `auto_nr4_t4` (europe_main) and the Peninsula saves (spain_main)):
//! - the picture is the theatre's `<x>_map.tga` (same size as the MAPS item);
//! - each pixel belongs to the region whose `regions` DB colour is the pixel's colour in
//!   `<x>_lookup.tga`;
//! - a pixel of a region the save's faction owns is tinted green: red and blue `⌊91 b / 256⌋`,
//!   green `⌊⌊(91 b + 165 × 255) / 256⌋ × 255 / 256⌋`;
//! - every other pixel is `⌊255 b / 256⌋` per channel;
//! - alpha is kept as the stored picture has it (0xFF).
//!
//! The original's own code for this (`GenerateRegionOwnershipMaps`) was not traced: the rule above
//! is read from its output. Only the faction's own regions are tinted: allies and protectorates are
//! not (none of them is tinted in the original's saves).

use std::collections::{HashMap, HashSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::tga::Tga;
use ntw_sim::campaign::CampaignModel;

/// The pictures of one theatre: the base map and the region lookup, with the lookup colour of
/// every region.
pub struct TheatrePictures {
    /// Theatre key, e.g. `europe_main`.
    pub theatre: String,
    /// `<x>_map.tga`.
    pub base: Tga,
    /// `<x>_lookup.tga`.
    pub lookup: Tga,
    /// Region key → lookup colour (`regions` DB colour).
    pub region_colours: HashMap<String, [u8; 3]>,
}

impl TheatrePictures {
    /// Loads a theatre's pictures from the campaign map folder (e.g. `campaign_maps/nap_europe`).
    /// `None` when a picture is missing.
    pub fn load(files: &ntw_formats::campaign_map::GameFiles<'_>, db: &ntw_data::GameDatabase, map_key: &str, theatre: &str) -> Option<TheatrePictures> {
        let folder = map_key.trim_start_matches("campaign_maps/").trim_start_matches("campaign_maps\\");
        let x = theatre.trim_end_matches("_main");
        let read = |name: &str| files.read(&format!("campaign_maps/{folder}/{name}")).ok().and_then(|b| Tga::decode(&b).ok());
        let base = read(&format!("{x}_map.tga"))?;
        let lookup = read(&format!("{x}_lookup.tga"))?;
        let region_colours = db.regions.rows().iter().map(|r| (r.key.clone(), r.colour())).collect();
        Some(TheatrePictures { theatre: theatre.to_owned(), base, lookup, region_colours })
    }

    /// The picture for a faction owning `owned` regions: 0xAARRGGBB pixels, rows top-down, alpha
    /// from `alpha` (the stored picture's) where given, else 0xFF.
    pub fn render(&self, owned: &HashSet<String>, alpha: Option<&[u32]>) -> Vec<u32> {
        let own_colours: HashSet<[u8; 3]> = owned.iter().filter_map(|k| self.region_colours.get(k).copied()).collect();
        let (w, h) = (self.base.width as usize, self.base.height as usize);
        let mut out = Vec::with_capacity(w * h);
        for i in 0..w * h {
            let b = &self.base.rgba[4 * i..4 * i + 3];
            let lc = if self.lookup.indices.is_empty() {
                let p = &self.lookup.rgba[4 * i..4 * i + 3];
                [p[0], p[1], p[2]]
            } else {
                let p = self.lookup.palette.get(self.lookup.indices.get(i).copied().unwrap_or(0) as usize).copied().unwrap_or([0; 4]);
                [p[0], p[1], p[2]]
            };
            let [r, g, bl] = if own_colours.contains(&lc) {
                let t = |v: u8| (v as u32 * 91) / 256;
                [t(b[0]), ((b[1] as u32 * 91 + 165 * 255) / 256) * 255 / 256, t(b[2])]
            } else {
                let p = |v: u8| v as u32 * 255 / 256;
                [p(b[0]), p(b[1]), p(b[2])]
            };
            let a = alpha.and_then(|a| a.get(i)).map_or(0xFF, |p| p >> 24);
            out.push((a << 24) | (r << 16) | (g << 8) | bl);
        }
        out
    }
}

/// Rewrites every `MAPS` item of the header whose theatre is in `pictures` (and of the same size)
/// from the model: the regions the save's faction (`human`) owns are tinted. Returns how many items
/// were rewritten.
pub fn update_maps(tree: &mut EsfFile, model: &CampaignModel, human: &str, pictures: &[TheatrePictures]) -> usize {
    let Some(hf) = model.faction_by_key(human).map(|f| f.id) else { return 0 };
    let owned: HashSet<String> = model.world.regions.values().filter(|r| r.owner == hf).map(|r| r.key.clone()).collect();
    let Some(header) = tree.root.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(r) if r.name == "SAVE_GAME_HEADER" => Some(&mut **r),
        _ => None,
    }) else {
        return 0;
    };
    let mut n = 0;
    for c in header.children.iter_mut() {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "MAPS" {
            continue;
        }
        for it in a.items.iter_mut() {
            let theatre = it.first().and_then(EsfNode::as_str).unwrap_or("").to_owned();
            let (w, h) = (it.get(1).and_then(EsfNode::as_u32).unwrap_or(0), it.get(2).and_then(EsfNode::as_u32).unwrap_or(0));
            let Some(p) = pictures.iter().find(|p| p.theatre == theatre && p.base.width == w && p.base.height == h && p.lookup.width == w && p.lookup.height == h) else { continue };
            let stored = it.get(4).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec);
            let pixels = p.render(&owned, stored.as_deref());
            if stored.as_deref() != Some(&pixels[..])
                && let Some(slot) = it.get_mut(4)
            {
                *slot = EsfNode::U32Array(pixels);
                n += 1;
            }
        }
    }
    n
}

/// `MAPS` items of a tree (theatre, pixels), for checks.
pub fn header_maps(root: &EsfRecord) -> Vec<(String, Vec<u32>)> {
    root.child("SAVE_GAME_HEADER")
        .and_then(|h| h.record_array("MAPS"))
        .map(|a| {
            a.items
                .iter()
                .map(|it| (it.first().and_then(EsfNode::as_str).unwrap_or("").to_owned(), it.get(4).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default()
}
