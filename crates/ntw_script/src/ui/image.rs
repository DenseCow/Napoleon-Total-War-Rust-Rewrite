//! Image objects of the UI scripts: `UIImage(path)` (a picture file, see `ui_prelude.lua`) and
//! `UIPaletisedImage(path)` (an 8-bit palette picture whose palette the script recolours).
//!
//! Evidence (template.map_image.lua, the radar and the theatre / region maps; CONFIRMED calls):
//! `image:Dimensions()` → width, height in pixels; `image:SetComponentTexture(address, 0)` shows it
//! on a component; `image:Release()`; for palette images also `image:Query(x, y)` → the palette
//! index under pixel (x, y) (FindRegion compares it with the regions' `PaletteEntry`) and
//! `image:SetPaletteEntry(i, r, g, b, a)` (i = 0..255). INFERRED: Query's origin is the top-left
//! pixel (the script measures x, y from the component's top-left corner).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Lua, Table, Value};
use ntw_formats::tga::Tga;

use super::host::Inner;
use super::world::RuntimeImage;

/// Width and height from an image file's header (TGA or DDS), without decoding the pixels.
fn header_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let u16_at = |o: usize| bytes.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32);
    let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if bytes.starts_with(b"DDS ") {
        return Some((u32_at(16)?, u32_at(12)?));
    }
    Some((u16_at(12)?, u16_at(14)?))
}

type Sizes = Rc<RefCell<HashMap<String, Option<(u32, u32)>>>>;

/// The `__ntw_image` table the prelude builds the image objects on.
pub(super) fn image_functions(lua: &Lua, inner: &Rc<Inner>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    // size(path) → width, height (nil if the file is missing). Cached: the scripts ask often.
    let sizes: Sizes = Rc::default();
    let i = inner.clone();
    t.set(
        "size",
        lua.create_function(move |_, path: String| {
            let found = *sizes
                .borrow_mut()
                .entry(path.to_ascii_lowercase())
                .or_insert_with(|| i.source.find(&path).and_then(|f| header_size(&f.bytes)));
            Ok((found.map(|s| s.0), found.map(|s| s.1)))
        })?,
    )?;
    // pal_load(path) → key, width, height (nil if the file is missing or has no palette).
    let next = Rc::new(Cell::new(0u32));
    let i = inner.clone();
    t.set(
        "pal_load",
        lua.create_function(move |_, path: String| {
            let none = (None::<String>, None::<u32>, None::<u32>);
            let Some(tga) = i.source.find(&path).and_then(|f| Tga::decode(&f.bytes).ok()) else { return Ok(none) };
            if tga.indices.is_empty() || tga.palette.is_empty() {
                return Ok(none);
            }
            let mut palette = tga.palette.clone();
            palette.resize(256, [0, 0, 0, 255]);
            next.set(next.get() + 1);
            let key = format!("palette{}", next.get());
            let (w, h) = (tga.width, tga.height);
            let img = RuntimeImage { width: w, height: h, indices: tga.indices, palette, ..Default::default() };
            let mut world = i.world.borrow_mut();
            world.runtime_images.insert(key.clone(), img);
            world.generation += 1;
            Ok((Some(key), Some(w), Some(h)))
        })?,
    )?;
    let i = inner.clone();
    t.set(
        "pal_query",
        lua.create_function(move |_, (key, x, y): (String, f64, f64)| {
            Ok(i.world.borrow().runtime_images.get(&key).and_then(|img| img.query(x.floor() as i64, y.floor() as i64)))
        })?,
    )?;
    let i = inner.clone();
    t.set(
        "pal_set",
        lua.create_function(move |_, (key, index, r, g, b, a): (String, f64, f64, f64, f64, Option<f64>)| {
            let c = |v: f64| v.clamp(0.0, 255.0) as u8;
            let mut world = i.world.borrow_mut();
            let Some(img) = world.runtime_images.get_mut(&key) else { return Ok(()) };
            if (0.0..256.0).contains(&index) {
                let e = [c(r), c(g), c(b), c(a.unwrap_or(255.0))];
                let slot = &mut img.palette[index as usize];
                if *slot != e {
                    *slot = e;
                    img.version += 1;
                    world.generation += 1;
                }
            }
            Ok(())
        })?,
    )?;
    let i = inner.clone();
    t.set(
        "pal_release",
        lua.create_function(move |_, key: String| {
            let mut world = i.world.borrow_mut();
            if world.runtime_images.remove(&key).is_some() {
                world.generation += 1;
            }
            Ok(Value::Nil)
        })?,
    )?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_sizes() {
        let mut tga = vec![0u8; 18];
        tga[12..14].copy_from_slice(&605u16.to_le_bytes());
        tga[14..16].copy_from_slice(&300u16.to_le_bytes());
        assert_eq!(header_size(&tga), Some((605, 300)));
        let mut dds = b"DDS ".to_vec();
        dds.extend_from_slice(&[0; 16]);
        dds[12..16].copy_from_slice(&64u32.to_le_bytes());
        dds[16..20].copy_from_slice(&128u32.to_le_bytes());
        assert_eq!(header_size(&dds), Some((128, 64)));
    }

    #[test]
    fn palette_image_queries_and_colours() {
        let img = RuntimeImage { width: 2, height: 1, indices: vec![3, 7], palette: vec![[0; 4]; 256], ..Default::default() };
        assert_eq!(img.query(1, 0), Some(7));
        assert_eq!(img.query(2, 0), None);
        assert_eq!(img.rgba().len(), 8);
    }
}
