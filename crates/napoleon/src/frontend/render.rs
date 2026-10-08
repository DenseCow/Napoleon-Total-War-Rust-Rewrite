//! Turns the live UI tree into Bevy sprites: one sprite per image metric of each visible
//! component's current state, plus one sprite per text (rasterised from the original `.cuf` font).
//!
//! Coordinates: the UI works in window pixels with the origin at the top-left and y down (like
//! the original). The 2D camera has its origin in the centre with y up, so we convert.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ntw_formats::dds::Dds;
use ntw_formats::font::{CufFont, font_file_for};
use ntw_formats::pack::Vfs;
use ntw_formats::tga::Tga;

use ntw_script::ui::{NodeId, RUNTIME_IMAGE_PREFIX, UiRect, UiWorld};
use crate::GameMode;

/// Marks every sprite the frontend drew (they are all rebuilt when the tree changes).
#[derive(Component)]
pub struct UiSprite;

/// A text drawn to a texture, and its size.
type RenderedText = (Handle<Image>, Vec2);

/// Loaded original files, shared by all frontend systems.
#[derive(Resource)]
pub struct UiAssets {
    vfs: Vfs,
    textures: HashMap<String, Option<Handle<Image>>>,
    fonts: HashMap<String, Option<CufFont>>,
    texts: HashMap<(String, String, u32), Option<RenderedText>>,
    /// Image files that are not in the packs, by normalised path (the UIEd template library's
    /// embedded images, see `UiScriptHost::template_images`).
    embedded: HashMap<String, Vec<u8>>,
    /// Movie pictures drawn over a component's rectangle, by component id (the front end's
    /// `movie_bg`, `crate::video`).
    pub movies: HashMap<String, Handle<Image>>,
    /// The install's data folder, for picture files outside the packs (the campaign maps' radar
    /// and lookup pictures are loose files in `data/campaign_maps/<map>/`).
    data_dir: Option<std::path::PathBuf>,
    /// Textures of the scripts' run-time images (see `ntw_script::ui::RuntimeImage`), by key,
    /// with the image version they were built from.
    runtime: HashMap<String, (u64, Handle<Image>)>,
}

impl UiAssets {
    pub fn new(vfs: Vfs) -> Self {
        Self { vfs, textures: HashMap::new(), fonts: HashMap::new(), texts: HashMap::new(), embedded: HashMap::new(), movies: HashMap::new(), data_dir: None, runtime: HashMap::new() }
    }

    /// Also reads picture files from the install's data folder when the packs do not have them.
    pub fn with_loose_files(mut self, data_dir: impl Into<std::path::PathBuf>) -> Self {
        self.data_dir = Some(data_dir.into());
        self
    }

    /// The texture of a run-time image (`ntw_script::ui::RUNTIME_IMAGE_PREFIX` + key), rebuilt
    /// when the script changed it.
    fn runtime_texture(&mut self, world: &UiWorld, key: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let img = world.runtime_images.get(key)?;
        if let Some((v, h)) = self.runtime.get(key)
            && *v == img.version
        {
            return Some(h.clone());
        }
        let h = match self.runtime.get(key) {
            Some((_, h)) if images.get(h).is_some() => {
                if let Some(mut tex) = images.get_mut(h) {
                    *tex = rgba_image(img.width, img.height, img.rgba());
                }
                h.clone()
            }
            _ => images.add(rgba_image(img.width, img.height, img.rgba())),
        };
        self.runtime.insert(key.to_owned(), (img.version, h.clone()));
        Some(h)
    }

    /// Adds image files that are not in the packs (path as the layouts store it, file bytes).
    pub fn add_embedded(&mut self, files: impl IntoIterator<Item = (String, Vec<u8>)>) {
        for (path, bytes) in files {
            let key = path.replace('/', "\\").to_ascii_lowercase();
            let key = key.strip_prefix("data\\").unwrap_or(&key).to_owned();
            self.embedded.entry(key).or_insert(bytes);
        }
    }

    /// A 1x1 white texture for untextured (painted) images.
    fn white(&mut self, images: &mut Assets<Image>) -> Handle<Image> {
        if let Some(Some(h)) = self.textures.get("") {
            return h.clone();
        }
        let h = images.add(rgba_image(1, 1, vec![255; 4]));
        self.textures.insert(String::new(), Some(h.clone()));
        h
    }

    /// Line height of a font (0 if it is missing).
    fn line_height(&mut self, font: &str) -> f32 {
        self.font(font).map_or(0.0, |f| f.line_height().max(f.ascent()) as f32)
    }

    /// `text` split into lines no wider than `width` (see `CufFont::wrap_lines`).
    fn wrap(&mut self, font: &str, text: &str, width: f32) -> Vec<String> {
        match self.font(font) {
            Some(f) => f.wrap_lines(text, width as i32),
            None => vec![text.to_owned()],
        }
    }

    /// A UI texture by the path stored in the layout (`UI/...` or `data\UI\...`).
    fn texture(&mut self, path: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let key = path.replace('/', "\\").to_ascii_lowercase();
        let key = key.strip_prefix("data\\").unwrap_or(&key).to_owned();
        if let Some(h) = self.textures.get(&key) {
            return h.clone();
        }
        let loaded = self.load_texture(&key).map(|img| images.add(img));
        if loaded.is_none() {
            warn!("UI texture not found or unreadable: {key}");
        }
        self.textures.insert(key, loaded.clone());
        loaded
    }

    fn load_texture(&self, key: &str) -> Option<Image> {
        let bytes = match self.embedded.get(key) {
            Some(b) => b.clone(),
            None => match self.vfs.read(key) {
                Ok(b) => b,
                Err(_) => std::fs::read(self.data_dir.as_ref()?.join(key.replace('\\', "/"))).ok()?,
            },
        };
        let (w, h, rgba) = if key.ends_with(".dds") {
            let dds = Dds::parse(&bytes).ok()?;
            let (w, h) = dds.level_dims(0);
            (w, h, dds.decode_rgba8(0))
        } else {
            let t = Tga::decode(&bytes).ok()?;
            (t.width, t.height, t.rgba)
        };
        Some(rgba_image(w, h, rgba))
    }

    fn font(&mut self, name: &str) -> Option<&CufFont> {
        if !self.fonts.contains_key(name) {
            let f = font_file_for(name).and_then(|p| self.vfs.read(&p).ok()).and_then(|b| CufFont::read(&b).ok());
            if f.is_none() {
                warn!("UI font not found: {name}");
            }
            self.fonts.insert(name.to_owned(), f);
        }
        self.fonts.get(name).and_then(Option::as_ref)
    }

    /// A single line of text rasterised with the original bitmap font, in the given ARGB colour.
    fn text(&mut self, font: &str, text: &str, argb: u32, images: &mut Assets<Image>) -> Option<(Handle<Image>, Vec2)> {
        let key = (font.to_owned(), text.to_owned(), argb);
        if let Some(t) = self.texts.get(&key) {
            return t.clone();
        }
        let made = self.font(font).and_then(|f| rasterise(f, text, argb)).map(|(img, size)| (images.add(img), size));
        self.texts.insert(key, made.clone());
        made
    }
}

fn rgba_image(w: u32, h: u32, rgba: Vec<u8>) -> Image {
    let mut img = Image::new(
        Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // UI art is pixel art at 1:1; linear filtering is what D3D9 UIs normally use for stretched pieces.
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

/// Draws `text` with a `.cuf` font into an RGBA image (PROVISIONAL: one line, no wrapping,
/// pen-advance rule from `CufFont::advance`).
fn rasterise(f: &CufFont, text: &str, argb: u32) -> Option<(Image, Vec2)> {
    let w = f.text_width(text).max(1) as usize;
    let h = f.line_height().max(f.ascent() + 4).max(1) as usize;
    let [b, g, r, a] = argb.to_le_bytes();
    let mut px = vec![0u8; w * h * 4];
    let chars: Vec<char> = text.chars().collect();
    let mut pen = 0i32;
    for (i, &ch) in chars.iter().enumerate() {
        if let Some((_, gl)) = f.glyph(ch)
            && gl.top != i8::MIN
        {
            let bmp = f.bitmap(gl);
            let top = f.ascent() - i32::from(gl.top);
            for gy in 0..i32::from(gl.height) {
                for gx in 0..i32::from(gl.width) {
                    let (x, y) = (pen + gx, top + gy);
                    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                        continue;
                    }
                    let cov = bmp[(gy * i32::from(gl.width) + gx) as usize];
                    let o = (y as usize * w + x as usize) * 4;
                    let alpha = (u32::from(cov) * u32::from(a) / 255) as u8;
                    if alpha > px[o + 3] {
                        px[o..o + 4].copy_from_slice(&[r, g, b, alpha]);
                    }
                }
            }
        }
        pen += f.advance(ch, chars.get(i + 1).copied());
    }
    Some((rgba_image(w as u32, h as u32, px), Vec2::new(w as f32, h as f32)))
}

fn argb_color(argb: u32) -> Color {
    let [b, g, r, a] = argb.to_le_bytes();
    Color::srgba_u8(r, g, b, a)
}

/// The original's UI scale for a window (0x0114EB20, CONFIRMED): min(1, width / 1280, height / 960),
/// the same on both axes. Normal (DrawMode 0) components are laid out in a virtual screen of
/// window / scale and drawn scaled by it (0x011617C0 sizes, 0x011881C0 positions: scale × position
/// + anchor × window, which is the same for docked components).
pub fn ui_scale(window: Vec2) -> f32 {
    (window.x / 1280.0).min(window.y / 960.0).min(1.0)
}

/// Converts a UI rectangle (virtual screen units) to a sprite transform (centre, y up) at depth
/// `z`, scaled by the UI scale `s`.
fn to_transform(r: UiRect, window: Vec2, z: f32, s: f32) -> Transform {
    Transform::from_xyz(s * (r.x + r.w / 2.0) - window.x / 2.0, window.y / 2.0 - s * (r.y + r.h / 2.0), z).with_scale(Vec3::new(s, s, 1.0))
}

/// Cuts a sprite to the clip rectangle of its component (see `UiWorld::clip_rect`): returns the
/// part of `rect` left to draw (None if nothing is) and points the sprite at the matching part of
/// its texture. PROVISIONAL for tiled images: the tiles restart at the clipped edge.
fn clip_sprite(sprite: &mut Sprite, rect: UiRect, clip: Option<UiRect>, images: &Assets<Image>) -> Option<UiRect> {
    let Some(clip) = clip else { return Some(rect) };
    let i = rect.intersect(&clip);
    if i.w <= 0.0 || i.h <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return None;
    }
    if (i.w - rect.w).abs() < 0.01 && (i.h - rect.h).abs() < 0.01 {
        return Some(rect);
    }
    sprite.custom_size = Some(Vec2::new(i.w, i.h));
    if matches!(sprite.image_mode, SpriteImageMode::Tiled { .. }) {
        return Some(i);
    }
    let size = images.get(&sprite.image)?.size_f32();
    let (mut u0, mut u1) = ((i.x - rect.x) / rect.w, (i.x + i.w - rect.x) / rect.w);
    let (mut v0, mut v1) = ((i.y - rect.y) / rect.h, (i.y + i.h - rect.y) / rect.h);
    if sprite.flip_x {
        (u0, u1) = (1.0 - u1, 1.0 - u0);
    }
    if sprite.flip_y {
        (v0, v1) = (1.0 - v1, 1.0 - v0);
    }
    sprite.rect = Some(Rect::new(u0 * size.x, v0 * size.y, u1 * size.x, v1 * size.y));
    Some(i)
}

/// Spawns the sprites for every visible component under `root`, in draw order. `scale` is the UI
/// scale (see [`ui_scale`]; 1.0 draws the layout 1:1 in window pixels).
pub fn spawn_world(world: &UiWorld, root: NodeId, commands: &mut Commands, assets: &mut UiAssets, images: &mut Assets<Image>, window: Vec2, scale: f32) {
    let mut z = 0.0f32;
    world.visit_visible(root, &mut |id, node| {
        let Some(state) = node.current() else { return };
        let clip = world.clip_rect(id);
        // A movie playing in this component fills its rectangle (stretched: INFERRED from the
        // exe's `MovieComponentPosAndDimensions`; `movie_bg` is 1920x960 for a 1280x720 movie).
        if let Some(movie) = assets.movies.get(&node.data.id) {
            z += 0.001;
            let sprite = Sprite { image: movie.clone(), custom_size: Some(Vec2::new(node.rect.w, node.rect.h)), ..default() };
            commands.spawn((sprite, to_transform(node.rect, window, z, scale), UiSprite, DespawnOnExit(GameMode::FrontEnd)));
        }
        for m in &state.image_metrics {
            // An image with an empty path (e.g. each layout root's backdrop) has no texture: not drawn,
            // unless a script painted it (root.lua paints the "layout" backdrop black).
            let Some(img) = node.data.image(m.image) else { continue };
            let handle = if img.path.is_empty() {
                if !node.painted {
                    continue;
                }
                assets.white(images)
            } else {
                let found = match img.path.strip_prefix(RUNTIME_IMAGE_PREFIX) {
                    Some(key) => assets.runtime_texture(world, key, images),
                    None => assets.texture(&img.path, images),
                };
                let Some(h) = found else { continue };
                h
            };
            let mut rect = UiRect { x: node.rect.x + m.offset.0 as f32, y: node.rect.y + m.offset.1 as f32, w: m.width as f32, h: m.height as f32 };
            // A component a script resized: its images keep their margins to the state's edges,
            // so pieces wider than half the state stretch and small end caps stay at their edge
            // (INFERRED: city_info_bar's three-piece name bar follows its Resize to the text).
            if node.resized {
                let (sw, sh) = (state.width as f32, state.height as f32);
                let fit = |off: f32, len: f32, old: f32, new: f32| -> (f32, f32) {
                    if old <= 0.0 || (new - old).abs() < 0.5 {
                        return (off, len);
                    }
                    let far = old - (off + len);
                    if len * 2.0 > old {
                        (off, (new - off - far).max(0.0))
                    } else if off > far {
                        (new - far - len, len)
                    } else {
                        (off, len)
                    }
                };
                let (x, w) = fit(m.offset.0 as f32, m.width as f32, sw, node.rect.w);
                let (y, h) = fit(m.offset.1 as f32, m.height as f32, sh, node.rect.h);
                rect = UiRect { x: node.rect.x + x, y: node.rect.y + y, w, h };
            }
            let mut sprite = Sprite {
                image: handle,
                custom_size: Some(Vec2::new(rect.w, rect.h)),
                color: argb_color(m.colour),
                flip_x: m.x_flipped,
                flip_y: m.y_flipped,
                ..default()
            };
            if m.tile {
                sprite.image_mode = SpriteImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 };
            }
            let Some(rect) = clip_sprite(&mut sprite, rect, clip, images) else { continue };
            z += 0.001;
            commands.spawn((sprite, to_transform(rect, window, z, scale), UiSprite, DespawnOnExit(GameMode::FrontEnd)));
        }
        // The text is already localised by the script host (as the engine does at load).
        let text = &state.text;
        if !text.trim().is_empty() {
            let r = node.rect;
            // The exe's text rules (ntw_formats::ui_layout::UiState::text_area / text_line_x /
            // text_block_y, CONFIRMED): the lines are broken inside the box minus the
            // TextX/YOffset insets, unless HBehaviour is NeverSplit, and placed by HAlign / VAlign
            // with the same insets. PROVISIONAL guard: only boxes tall enough for two lines are
            // broken, because our stand-in font metrics differ slightly from the game's and would
            // otherwise wrap single-line labels.
            let (area_w, _) = state.text_area(r.w, r.h);
            let lh = assets.line_height(&state.font);
            let wraps = state.text_wraps() && lh > 0.0 && r.h >= 2.0 * lh;
            let lines = if wraps { assets.wrap(&state.font, text, area_w) } else { vec![text.clone()] };
            let block_h = lh * lines.len() as f32;
            let top = r.y + state.text_block_y(r.h, block_h);
            for (n, line) in lines.iter().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let Some((handle, size)) = assets.text(&state.font, line, state.font_colour, images) else { continue };
                let x = r.x + state.text_line_x(r.w, size.x);
                // One line: placed by its own rendered height (the exe's measured DisplayHeight).
                let y = if lines.len() == 1 { r.y + state.text_block_y(r.h, size.y) } else { top + lh * n as f32 };
                let rect = UiRect { x: x.round(), y: y.round(), w: size.x, h: size.y };
                let mut sprite = Sprite { image: handle, custom_size: Some(size), ..default() };
                let Some(rect) = clip_sprite(&mut sprite, rect, clip, images) else { continue };
                z += 0.001;
                commands.spawn((
                    sprite,
                    to_transform(rect, window, z, scale),
                    UiSprite,
                    DespawnOnExit(GameMode::FrontEnd),
                ));
            }
        }
    });
}
