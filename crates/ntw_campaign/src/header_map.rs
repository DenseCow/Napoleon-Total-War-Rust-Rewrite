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

/// A theatre area key's short name (`europe_main` → `europe`): the stem of its pictures' file names
/// and the campaign HUD's theatre id (the radar button, the governorship key; PROVISIONAL there).
pub fn theatre_stem(theatre: &str) -> &str {
    theatre.trim_end_matches("_main")
}

/// The pictures of one theatre: the base map and the region lookup, with the lookup colour of
/// every region. Both pictures have the same size and complete pixel data ([`TheatrePictures::new`]
/// checks it), so rendering never reads past either.
pub struct TheatrePictures {
    theatre: String,
    base: Tga,
    lookup: Tga,
    region_colours: HashMap<String, [u8; 3]>,
}

impl TheatrePictures {
    /// A theatre's pictures: `base` (`<x>_map.tga`), `lookup` (`<x>_lookup.tga`) and region key →
    /// lookup colour (`regions` DB colour). An error when the two pictures differ in size or a
    /// picture's pixel data does not cover it (the original's header writer used only pictures
    /// of the header picture's size).
    pub fn new(theatre: &str, base: Tga, lookup: Tga, region_colours: HashMap<String, [u8; 3]>) -> Result<TheatrePictures, String> {
        let n = base.width as usize * base.height as usize;
        if (lookup.width, lookup.height) != (base.width, base.height) {
            return Err(format!("{theatre}: the lookup picture is {}x{}, the map {}x{}", lookup.width, lookup.height, base.width, base.height));
        }
        let lookup_complete = if lookup.indices.is_empty() { lookup.rgba.len() >= 4 * n } else { lookup.indices.len() >= n };
        if base.rgba.len() < 4 * n || !lookup_complete {
            return Err(format!("{theatre}: a picture has fewer pixels than its size"));
        }
        Ok(TheatrePictures { theatre: theatre.to_owned(), base, lookup, region_colours })
    }

    /// Loads a theatre's pictures from the campaign map folder (e.g. `campaign_maps/nap_europe`).
    /// `None`, logged (once per load), when a picture is missing, unreadable or the two do not fit
    /// (the save header then has no picture for that theatre).
    pub fn load(files: &ntw_formats::campaign_map::GameFiles<'_>, db: &ntw_data::GameDatabase, map_key: &str, theatre: &str) -> Option<TheatrePictures> {
        let folder = map_key.trim_start_matches("campaign_maps/").trim_start_matches("campaign_maps\\");
        let x = theatre_stem(theatre);
        let read = |name: &str| -> Option<Tga> {
            let path = format!("campaign_maps/{folder}/{name}");
            let bytes = files.read(&path).map_err(|e| log::warn!("Save header picture of {theatre}: {e}")).ok()?;
            Tga::decode(&bytes).map_err(|e| log::warn!("Save header picture {path}: {e:?}")).ok()
        };
        let base = read(&format!("{x}_map.tga"))?;
        let lookup = read(&format!("{x}_lookup.tga"))?;
        let region_colours = db.regions.rows().iter().map(|r| (r.key.clone(), r.colour())).collect();
        TheatrePictures::new(theatre, base, lookup, region_colours).map_err(|e| log::warn!("Save header picture of {e}; left out")).ok()
    }

    /// The theatre key, e.g. `europe_main`.
    pub fn theatre(&self) -> &str {
        &self.theatre
    }

    /// The pictures' width and height.
    pub fn size(&self) -> (u32, u32) {
        (self.base.width, self.base.height)
    }

    /// The lookup picture's colour at pixel `i` (row-major, top row first): the colour of the region
    /// the pixel belongs to.
    pub fn lookup_colour(&self, i: usize) -> [u8; 3] {
        if self.lookup.indices.is_empty() {
            let p = &self.lookup.rgba[4 * i..4 * i + 3];
            [p[0], p[1], p[2]]
        } else {
            let p = self.lookup.palette.get(self.lookup.indices.get(i).copied().unwrap_or(0) as usize).copied().unwrap_or([0; 4]);
            [p[0], p[1], p[2]]
        }
    }

    /// A region's lookup colour (its `regions` DB colour).
    pub fn region_colour(&self, key: &str) -> Option<[u8; 3]> {
        self.region_colours.get(key).copied()
    }

    /// The picture for a faction owning `owned` regions: 0xAARRGGBB pixels, rows top-down, alpha
    /// from `alpha` (the stored picture's) where given, else 0xFF.
    pub fn render(&self, owned: &HashSet<String>, alpha: Option<&[u32]>) -> Vec<u32> {
        let own_colours: HashSet<[u8; 3]> = owned.iter().filter_map(|k| self.region_colours.get(k).copied()).collect();
        let (w, h) = (self.base.width as usize, self.base.height as usize);
        let mut out = Vec::with_capacity(w * h);
        for i in 0..w * h {
            let b = &self.base.rgba[4 * i..4 * i + 3];
            let lc = self.lookup_colour(i);
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

/// The keys of the regions `human` (a faction key) owns: the ones its pictures tint. Empty for an
/// unknown faction.
pub fn owned_regions(model: &CampaignModel, human: &str) -> HashSet<String> {
    let Some(hf) = model.faction_by_key(human).map(|f| f.id) else { return HashSet::new() };
    model.world.regions.values().filter(|r| r.owner == hf).map(|r| r.key.clone()).collect()
}

/// Rewrites every `MAPS` item of the header whose theatre is in `pictures` (and of the same size)
/// from the model: the regions the save's faction (`human`) owns are tinted. Returns how many items
/// were rewritten.
pub fn update_maps(tree: &mut EsfFile, model: &CampaignModel, human: &str, pictures: &[TheatrePictures]) -> usize {
    if model.faction_by_key(human).is_none() {
        return 0;
    }
    let owned = owned_regions(model, human);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A true-colour picture of `w` × `h` pixels, all `rgb`.
    fn picture(w: u32, h: u32, rgb: [u8; 3]) -> Tga {
        Tga { width: w, height: h, rgba: [rgb[0], rgb[1], rgb[2], 255].repeat((w * h) as usize), indices: Vec::new(), palette: Vec::new() }
    }

    /// A map mod whose lookup picture is smaller than its map picture (or whose pixel data is short)
    /// gives no theatre pictures, so the header writer never reads past a picture (it panicked on
    /// F5 before).
    #[test]
    fn pictures_that_do_not_fit_are_refused() {
        let colours = HashMap::from([("made_up_region".to_string(), [10, 20, 30])]);
        assert!(TheatrePictures::new("made_up_main", picture(4, 2, [200; 3]), picture(2, 2, [10, 20, 30]), colours.clone()).is_err());
        let mut short = picture(4, 2, [10, 20, 30]);
        short.rgba.truncate(12);
        assert!(TheatrePictures::new("made_up_main", picture(4, 2, [200; 3]), short, colours.clone()).is_err());
        let p = TheatrePictures::new("made_up_main", picture(4, 2, [200; 3]), picture(4, 2, [10, 20, 30]), colours).expect("pictures that fit");
        assert_eq!((p.theatre(), p.size()), ("made_up_main", (4, 2)));
        let owned = HashSet::from(["made_up_region".to_string()]);
        let px = p.render(&owned, None);
        assert_eq!(px.len(), 8);
        // Owned: red and blue ⌊91 · 200 / 256⌋ = 71, green ⌊⌊(91 · 200 + 165 · 255) / 256⌋ · 255 / 256⌋ = 234.
        assert!(px.iter().all(|&c| c == 0xFF47_EA47), "{px:x?}");
    }
}
