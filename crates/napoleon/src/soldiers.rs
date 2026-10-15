//! Soldier figures from the install: assembled, textured, faction-tinted and animated.
//!
//! The data side lives in [`ntw_formats::unit_model`] (unit key -> uniforms -> variant parts,
//! equipment, textures, colours), [`ntw_formats::anim`] (skeleton + clips),
//! [`ntw_formats::unit_animation`] (which figures, mounts and clips, from the DB) and
//! [`ntw_formats::mount`] (horse models). This module turns one figure (a man, and his horse if
//! he rides) into Bevy meshes and materials, and re-poses those meshes on the CPU each time
//! the animation reaches a new frame. A rider shares his mount's origin: the paired rider and
//! mount clips are authored in one space (`analysis/units/CAVALRY.md` §3).
//!
//! Skinning is done on the CPU because the original format stores, per vertex, a separate
//! bone-local position for each of its two bones (no bind pose, so Bevy's GPU skinning
//! cannot express it exactly). Every man of a unit shares the same meshes, so the work is
//! per unit kit, not per man. PROVISIONAL: the original almost certainly skins on the GPU
//! and gives each man his own animation phase.
//!
//! Coordinates: the files are left-handed, Y up, a man faces +Z. Like the model viewer we
//! negate Z (so a man faces Bevy's forward, -Z) and swap the triangle winding.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat};
use ntw_data::schemas::UnitStatsLand;
use ntw_formats::anim::Anim;
use ntw_formats::battle_animation::AnimationTables;
use ntw_formats::dds::Dds;
use ntw_formats::mount::{MountIndex, MountModel};
use ntw_formats::pack::Vfs;
use ntw_formats::unit_animation::{self, FigurePlan, Gait, UnitAnimationKeys, choose_clips, gait_levels, gait_speeds, plan_figure};
use ntw_formats::unit_model::{
    BattleTables, EquipmentLibrary, EquipmentThemes, PartBinding, SoldierModel, SoldierPart, UniformColours,
    UnitModelIndex, VariantRole, tint, unit_variant_path,
};
use ntw_formats::weighted_mesh::WeightedPiece;
use ntw_formats::unit_variant::UnitVariant;

/// Everything needed to build soldiers and their mounts, opened once.
#[derive(Resource)]
pub struct SoldierLibrary {
    pub vfs: Vfs,
    pub index: UnitModelIndex,
    pub equipment: EquipmentLibrary,
    pub themes: EquipmentThemes,
    /// `animation_tables.txt` and its fragments (clip selection).
    pub tables: AnimationTables,
    /// `battle_personalities` + `battle_entities`.
    pub battle: BattleTables,
    /// `mount_variants` + `warscape_animated[_lod]`.
    pub mounts: MountIndex,
    /// path -> (clip with its root drift removed, the clip's own root speed in m/s).
    clips: HashMap<String, Option<(Arc<Anim>, f32)>>,
    images: HashMap<String, Handle<Image>>,
}

/// One assembled man as Bevy assets: one mesh + material per part.
#[derive(Clone)]
pub struct SoldierKit {
    pub parts: Vec<KitPart>,
    /// Frame last written into the meshes, to skip re-posing at the same frame.
    pub posed: Option<(usize, usize)>,
}

#[derive(Clone)]
pub struct KitPart {
    pub part: Arc<SoldierPart>,
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

/// One assembled mount (horse, camel) as Bevy assets: one mesh per chosen piece.
#[derive(Clone)]
pub struct MountKit {
    /// The `warscape_animated` model key and mesh file (for logs).
    pub model_key: String,
    pub mesh_path: String,
    pub pieces: Vec<(Arc<WeightedPiece>, Handle<Mesh>)>,
    pub material: Handle<StandardMaterial>,
    pub posed: Option<(usize, usize)>,
    /// The less detailed LODs (same pieces in the same order) and every LOD's
    /// `warscape_animated_lod` distance (`mount::animated_lod`).
    pub lower_lods: Vec<Vec<Arc<WeightedPiece>>>,
    pub lod_distances: Vec<f32>,
}

/// The clips of one figure for one gait: the man's (rider's) and the mount's paired clip.
#[derive(Clone)]
pub struct GaitAnims {
    pub man: Arc<Anim>,
    /// The man clip's root speed (m/s) as read: `man` loops in place ([`in_place`]), so its own is 0.
    pub speed: f32,
    pub mount: Option<Arc<Anim>>,
    /// Slot names, for logs (`RIDER_TROT` / `TROT`).
    pub slots: (String, Option<String>),
    /// The slot kind the clip plays by ([`slot_kind`]: the mount's slot when mounted).
    pub kind: u8,
}

/// The kind of action slot `slot` (`battle_animation::SLOT_KINDS`), which picks how the display
/// advances a clip of it and whether a change into it carries the phase (`view::ClipBlend`). The
/// slots the kit asks for are the exe's own, so a name outside the table is a bug in our lists:
/// logged once per name (every kit build asks again), and played as kind 0 (frame time, no
/// phase carry).
fn slot_kind(slot: &str) -> u8 {
    /// The names already logged (cold path: kit builds only; a few names at most).
    static LOGGED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    ntw_formats::battle_animation::slot_kind(slot).unwrap_or_else(|| {
        // A poisoned lock only loses the dedup: log again rather than not at all.
        let first = LOGGED.lock().map_or(true, |mut logged| {
            let first = !logged.iter().any(|s| s == slot);
            if first {
                logged.push(slot.to_owned());
            }
            first
        });
        if first {
            warn!("animation slot {slot} is not in the exe's slot table; played as kind 0");
        }
        0
    })
}

/// The slot kind of a figure's level ([`slot_kind`]): the mount's slot when it has one, as the
/// rider copies his mount display's clip time (`0x0079BB10`; the rider's own kinds 0x16..0x1A,
/// his refusals, combat, loops and turns, copy it at `0x00774282`), else the man's.
fn level_kind(man_slot: &str, mount_slot: Option<&str>) -> u8 {
    slot_kind(mount_slot.unwrap_or(man_slot))
}

/// Every alternative clip of one speed level of a gait (see
/// `ntw_formats::unit_animation::gait_levels`). The battle picks a level by speed and an
/// alternative by each man's selection number.
#[derive(Clone)]
pub struct KitLevel {
    /// The man's (rider's) alternatives.
    pub man: Vec<Arc<Anim>>,
    /// Each of `man`'s clips' root speed (m/s, `Anim::root_speed`, the exe's anim `+0x60`), read
    /// before its drift was removed (same length and order as `man`).
    pub speeds: Vec<f32>,
    /// Each of `man`'s fragment lines' blend-in time (s, `FragmentClip::blend_in`; same length
    /// and order as `man`): how long a change into that clip cross-fades (`view::ClipBlend`).
    pub blend_in: Vec<f32>,
    /// The mount's alternatives (empty on foot).
    pub mount: Vec<Arc<Anim>>,
    /// Each of `mount`'s clips' root speed (m/s; same length and order as `mount`).
    pub mount_speeds: Vec<f32>,
    /// Each of `mount`'s lines' blend-in time (s; the mount fragment's lines, same length and
    /// order as `mount`): the mount display blends its own clip changes (`view::ClipBlend`).
    pub mount_blend_in: Vec<f32>,
    /// The slot kind the level's clips play by ([`level_kind`]: the mount's slot when mounted).
    pub kind: u8,
}

impl KitLevel {
    /// The root speed (m/s) of the clip a figure with selection number `sel` plays from this
    /// level: its own alternative's (the mount's when mounted, as the rider copies his mount
    /// display's clip time, `0x0079BB10`), 0 for a level without clips. CONFIRMED (`0x007725D0`):
    /// the playback rate (`0x00772E2B`), the phase carry at a clip change (`0x00773E85`..
    /// `0x00773E94`) and the locomotion graph's up / down edges and entry test (`0x007CEAA0`,
    /// `0x007CE840`, `0x00703120`) all read the anim `+0x60` of the display's own clip, the slot's
    /// alternative `selection % count` (`0x00E5F760`), never the slot's first clip's.
    pub fn speed_of(&self, sel: u32) -> f32 {
        let speeds = if self.mount.is_empty() { &self.speeds } else { &self.mount_speeds };
        speeds.get(ntw_formats::unit_animation::alternative(sel, speeds.len())).copied().unwrap_or(0.0)
    }

    /// The blend-in time of the alternative a man with selection number `sel` plays.
    pub fn blend_in_of(&self, sel: u32) -> f32 {
        self.blend_in[ntw_formats::unit_animation::alternative(sel, self.man.len())]
    }

    /// The blend-in time of the mount alternative a figure with selection number `sel` rides
    /// (0 on foot).
    pub fn mount_blend_in_of(&self, sel: u32) -> f32 {
        let i = ntw_formats::unit_animation::alternative(sel, self.mount.len());
        self.mount_blend_in.get(i).copied().unwrap_or(0.0)
    }
}

/// A whole figure: the man, his mount if he rides, and his clips per gait, all chosen
/// through the DB (`ntw_formats::unit_animation`).
#[derive(Clone)]
pub struct FigureKit {
    pub plan: FigurePlan,
    pub man: SoldierKit,
    pub mount: Option<MountKit>,
    /// One clip pair per gait, matched to the unit's DB speeds (the model viewer plays these).
    pub gaits: Vec<(Gait, GaitAnims)>,
    /// Every speed level and alternative per gait, slowest level first.
    pub levels: Vec<(Gait, Vec<KitLevel>)>,
    /// Action clips by slot name (fire, reload, melee, deaths, knock-downs; see
    /// `SoldierLibrary::action_clips`).
    pub actions: HashMap<String, KitLevel>,
    /// The stand clip is a `_TRAINED` one (trained death families).
    pub trained: bool,
    /// A mounted figure's gait ladder (`unit_animation::MOUNT_LADDER`), slowest loop first, as
    /// far as its tables resolve the loops; empty on foot.
    pub ladder: Vec<LadderRung>,
}

/// One rung of a mounted figure's gait ladder: its loop, and the transition clip into it from
/// the rung below (from the stand for the first), when the tables have one.
#[derive(Clone)]
pub struct LadderRung {
    pub into: Option<KitLevel>,
    pub gait: KitLevel,
}

/// Bevy asset stores used while building kits.
pub struct KitAssets<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
}

/// The animation-related `unit_stats_land` columns of a unit.
pub fn animation_keys(s: &UnitStatsLand) -> UnitAnimationKeys {
    UnitAnimationKeys {
        man_animation_type: s.man_animation_type.clone(),
        man_entity: s.man_entity.clone(),
        weapon_theme: Some(s.weapon_anim_group.clone()).filter(|t| !t.is_empty()),
        officer: Some(s.officer.clone()).filter(|t| !t.is_empty()),
        musician: s.musician.clone(),
        standard_bearer: s.standard_bearer.clone(),
        num_mounts: s.num_mounts,
        mount: s.mount.clone(),
        mount_entity: s.mount_entity.clone(),
        mount_type: s.mount_type.clone(),
    }
}

impl SoldierLibrary {
    pub fn open(data_dir: &std::path::Path) -> Result<Self, String> {
        let vfs = Vfs::open_install(data_dir).map_err(|e| e.to_string())?;
        let index = UnitModelIndex::from_vfs(&vfs).map_err(|e| e.to_string())?;
        let equipment = EquipmentLibrary::from_vfs(&vfs).map_err(|e| e.to_string())?;
        let themes = EquipmentThemes::from_vfs(&vfs).map_err(|e| e.to_string())?;
        let tables = AnimationTables::from_vfs(&vfs).map_err(|e| e.to_string())?;
        let battle = BattleTables::from_vfs(&vfs).map_err(|e| e.to_string())?;
        let mounts = MountIndex::from_vfs(&vfs).map_err(|e| e.to_string())?;
        Ok(Self {
            vfs,
            index,
            equipment,
            themes,
            tables,
            battle,
            mounts,
            clips: HashMap::new(),
            images: HashMap::new(),
        })
    }

    fn clip_entry(&mut self, path: &str) -> Option<(Arc<Anim>, f32)> {
        let key = path.to_ascii_lowercase().replace('\\', "/");
        if let Some(c) = self.clips.get(&key) {
            return c.clone();
        }
        let loaded = self.vfs.read(path).ok().and_then(|b| Anim::read(&b).ok()).map(|a| {
            let speed = a.root_speed();
            (Arc::new(in_place(a)), speed)
        });
        if loaded.is_none() {
            warn!("animation clip {path} could not be read");
        }
        self.clips.insert(key, loaded.clone());
        loaded
    }

    /// A clip from the packs with its root drift removed (loops in place), cached.
    pub fn clip(&mut self, path: &str) -> Option<Arc<Anim>> {
        self.clip_entry(path).map(|c| c.0)
    }

    /// A clip with its root motion kept (one-shots: a falling man moves within his spot, as the
    /// engine moves men by their clips' root motion, UNITS_TERRAIN_FIDELITY.md §1.6), cached,
    /// with its root speed.
    pub fn clip_raw(&mut self, path: &str) -> Option<(Arc<Anim>, f32)> {
        let key = format!("raw:{}", path.to_ascii_lowercase().replace('\\', "/"));
        if let Some(c) = self.clips.get(&key) {
            return c.clone();
        }
        let loaded = self.vfs.read(path).ok().and_then(|b| Anim::read(&b).ok()).map(|a| {
            let speed = a.root_speed();
            (Arc::new(a), speed)
        });
        if loaded.is_none() {
            warn!("animation clip {path} could not be read");
        }
        self.clips.insert(key, loaded.clone());
        loaded
    }

    /// A level's alternative clips (`(path, blend-in time)`, fragment order) with their own root
    /// speeds and blend-in times, the three lists in the same order; a clip that does not read is
    /// left out (logged once by the clip cache). `looping` clips loop in place
    /// ([`Self::clip`]), the others keep their root motion ([`Self::clip_raw`]).
    fn alternatives<'p>(&mut self, clips: impl IntoIterator<Item = (&'p str, f32)>, looping: bool) -> (Vec<Arc<Anim>>, Vec<f32>, Vec<f32>) {
        let (mut anims, mut speeds, mut blend_in) = (Vec::new(), Vec::new(), Vec::new());
        for (path, blend) in clips {
            if let Some((a, speed)) = if looping { self.clip_entry(path) } else { self.clip_raw(path) } {
                anims.push(a);
                speeds.push(speed);
                blend_in.push(blend);
            }
        }
        (anims, speeds, blend_in)
    }

    /// The action clips (fire, reload, melee, death, knock-down) of a planned figure: every
    /// action slot (`unit_animation::foot_action_slots` / `rider_action_slots`) its table
    /// resolves, with the paired mount slot when mounted. Loops keep their drift removed,
    /// one-shots keep their root motion.
    fn action_clips(&mut self, tables: &AnimationTables, plan: &FigurePlan) -> HashMap<String, KitLevel> {
        let mut out = HashMap::new();
        let slots = if plan.mount.is_some() { unit_animation::rider_action_slots() } else { unit_animation::foot_action_slots() };
        for slot in slots {
            let resolved = tables.resolve(&plan.animation_table, &slot);
            if resolved.is_empty() {
                continue;
            }
            // Each clip keeps its own root speed (loops too: the charge loop plays at the man's
            // speed over it, `KitLevel::speed_of`).
            let looping = slot == unit_animation::COMBAT_READY || slot.contains("COMBAT_IDLE") || slot == unit_animation::AIM || slot == unit_animation::CHARGE;
            let (man, speeds, blend_in) = self.alternatives(resolved.iter().map(|c| (c.clip.filename.as_str(), c.clip.blend_in())), looping);
            let mount_slot = plan.mount.as_ref().and(unit_animation::rider_mount_slot(&slot));
            let (mount, mount_speeds, mount_blend_in) = match (&plan.mount, &mount_slot) {
                (Some(m), Some(ms)) => {
                    let horse = tables.resolve(&m.animation_table, ms);
                    self.alternatives(horse.iter().map(|c| (c.clip.filename.as_str(), c.clip.blend_in())), looping)
                }
                _ => Default::default(),
            };
            if !man.is_empty() {
                let kind = level_kind(&slot, mount_slot.as_deref().filter(|_| !mount.is_empty()));
                out.insert(slot, KitLevel { man, speeds, blend_in, mount, mount_speeds, mount_blend_in, kind });
            }
        }
        out
    }

    /// Builds one figure of a unit: plans it from the DB (`keys`, `role`), chooses its
    /// stand / walk / run clips, assembles the man with the equipment the stand clip's
    /// fragment displays, and the mount if he rides. `seed` varies parts, coat colour and
    /// clip alternatives per man.
    pub fn build_figure(
        &mut self,
        unit: &str,
        faction: Option<&str>,
        keys: &UnitAnimationKeys,
        role: VariantRole,
        seed: u64,
        assets: &mut KitAssets<'_>,
    ) -> Result<FigureKit, String> {
        let plan = plan_figure(keys, role, &self.battle, &self.tables)
            .ok_or_else(|| format!("{unit} {role:?}: no animation table / personality"))?;
        let speeds = gait_speeds(&plan, &self.battle);
        let tables = std::mem::take(&mut self.tables);
        let mut gaits = Vec::new();
        let mut display = Vec::new();
        for gait in Gait::ALL {
            let mut speed_of = |p: &str| self.clip_entry(p).map(|c| c.1);
            let Some(chosen) = choose_clips(&tables, &plan, gait, speeds, seed as usize, &mut speed_of) else {
                warn!("{unit} {role:?}: no {gait:?} clip in table {}", plan.animation_table);
                continue;
            };
            if gait == Gait::Stand {
                display = chosen.man.equipment_display.clone();
            }
            let man = self.clip_entry(&chosen.man.path);
            let mount = chosen.mount.as_ref().and_then(|m| self.clip(&m.path));
            if let Some((man, speed)) = man {
                let slots = (chosen.man.slot.clone(), chosen.mount.as_ref().map(|m| m.slot.clone()));
                gaits.push((gait, GaitAnims { man, speed, mount, kind: level_kind(&slots.0, slots.1.as_deref()), slots }));
            }
        }
        // Every level and alternative, for the battle's per-man / per-frame choice.
        let mut levels = Vec::new();
        for gait in Gait::ALL {
            let mut speed_of = |p: &str| self.clip_entry(p).map(|c| c.1);
            let found = gait_levels(&tables, &plan, gait, &mut speed_of);
            let mut kit_levels = Vec::new();
            for level in found {
                let (man, speeds, blend_in) = self.alternatives(level.man.iter().map(|c| (c.path.as_str(), c.blend_in_time)), true);
                let (mount, mount_speeds, mount_blend_in) = self.alternatives(level.mount.iter().flatten().map(|c| (c.path.as_str(), c.blend_in_time)), true);
                if man.is_empty() || (plan.mount.is_some() && mount.is_empty()) {
                    continue;
                }
                // `man` read a clip, so the level has one: its kind is its first line's slot's.
                let kind = level_kind(&level.man[0].slot, level.mount.as_ref().and_then(|m| m.first()).map(|m| m.slot.as_str()));
                kit_levels.push(KitLevel { man, speeds, blend_in, mount, mount_speeds, mount_blend_in, kind });
            }
            if !kit_levels.is_empty() {
                levels.push((gait, kit_levels));
            }
        }
        let actions = self.action_clips(&tables, &plan);
        let ladder = self.ladder(&tables, &plan);
        self.tables = tables;
        if gaits.is_empty() {
            return Err(format!("{unit} {role:?}: no clips in table {}", plan.animation_table));
        }
        let man = self.build_kit_with(unit, faction, plan.equipment_theme.as_deref(), &display, role, seed, assets)?;
        let mount = match &plan.mount {
            Some(m) => match self.build_mount(&m.mount, seed, assets) {
                Ok(k) => Some(k),
                Err(e) => {
                    warn!("{unit}: mount {}: {e}", m.mount);
                    None
                }
            },
            None => None,
        };
        let trained = gaits.iter().any(|(g, a)| *g == Gait::Stand && a.slots.0.contains("TRAINED"));
        Ok(FigureKit { plan, man, mount, gaits, levels, actions, trained, ladder })
    }

    /// The rider's and the mount's clips of mount slot `mount_slot` (the rider's through
    /// `unit_animation::rider_slots`), in place, with their lines' blend-in times and their own
    /// root speeds. None on foot or when either does not resolve.
    fn mounted_level(&mut self, tables: &AnimationTables, plan: &FigurePlan, mount_slot: &str) -> Option<KitLevel> {
        let m = plan.mount.as_ref()?;
        let rider = unit_animation::rider_slots(mount_slot).iter().map(|s| tables.resolve(&plan.animation_table, s)).find(|v| !v.is_empty())?;
        let horse = tables.resolve(&m.animation_table, mount_slot);
        let (man, speeds, blend_in) = self.alternatives(rider.iter().map(|c| (c.clip.filename.as_str(), c.clip.blend_in())), true);
        let (mount, mount_speeds, mount_blend_in) = self.alternatives(horse.iter().map(|c| (c.clip.filename.as_str(), c.clip.blend_in())), true);
        (!man.is_empty() && !mount.is_empty()).then_some(KitLevel { man, speeds, blend_in, mount, mount_speeds, mount_blend_in, kind: slot_kind(mount_slot) })
    }

    /// A mounted figure's gait ladder: the rungs of `unit_animation::MOUNT_LADDER` whose loop
    /// resolves, up to the first that does not.
    fn ladder(&mut self, tables: &AnimationTables, plan: &FigurePlan) -> Vec<LadderRung> {
        let mut out = Vec::new();
        for (into, gait) in unit_animation::MOUNT_LADDER {
            let Some(gait) = self.mounted_level(tables, plan, gait) else { break };
            let into = self.mounted_level(tables, plan, into);
            out.push(LadderRung { into, gait });
        }
        out
    }

    /// Assembles a mount (`mounts` key) as Bevy meshes with its texture.
    pub fn build_mount(&mut self, mount: &str, seed: u64, assets: &mut KitAssets<'_>) -> Result<MountKit, String> {
        let model = MountModel::assemble(&self.vfs, &self.mounts, mount, seed).map_err(|e| e.to_string())?;
        let texture = model.textures.diffuse.as_ref().and_then(|d| {
            if let Some(h) = self.images.get(d) {
                return Some(h.clone());
            }
            match load_tinted(&self.vfs, d, None, None) {
                Ok(img) => {
                    let h = assets.images.add(img);
                    self.images.insert(d.clone(), h.clone());
                    Some(h)
                }
                Err(e) => {
                    warn!("{d}: {e}");
                    None
                }
            }
        });
        let material = assets.materials.add(StandardMaterial {
            base_color_texture: texture.clone(),
            base_color: if texture.is_some() { Color::WHITE } else { Color::srgb(0.4, 0.3, 0.2) },
            perceptual_roughness: 0.75,
            reflectance: 0.2,
            alpha_mode: AlphaMode::Mask(0.5),
            ..default()
        });
        let lower_lods = model.lower_lods.into_iter().map(|l| l.into_iter().map(Arc::new).collect()).collect();
        let lod_distances = model.lod_distances;
        let pieces = model
            .pieces
            .into_iter()
            .map(|p| {
                let uvs: Vec<[f32; 2]> = p.vertices.iter().map(|v| v.uv).collect();
                let mesh = assets.meshes.add(empty_mesh_from(uvs, p.indices.iter().copied()));
                (Arc::new(p), mesh)
            })
            .collect();
        Ok(MountKit { model_key: model.model_key, mesh_path: model.mesh_path, pieces, material, posed: None, lower_lods, lod_distances })
    }

    /// Assembles one man of `unit` (a `units` key). `faction` picks the uniform row;
    /// `theme` (an equipment theme) supplies the equipment, showing only the sets in
    /// `display` (an animation fragment's `default_equipment_display`; empty = primary +
    /// ambient + instrument); `seed` varies parts per man.
    #[allow(clippy::too_many_arguments)]
    pub fn build_kit_with(
        &mut self,
        unit: &str,
        faction: Option<&str>,
        theme: Option<&str>,
        display: &[String],
        role: VariantRole,
        seed: u64,
        assets: &mut KitAssets<'_>,
    ) -> Result<SoldierKit, String> {
        let uniform = self
            .index
            .uniforms_for_unit(unit, faction)
            .first()
            .map(|u| (*u).clone())
            .ok_or_else(|| format!("unit {unit} has no uniforms row"))?;
        let colours = self.index.colours(&uniform);
        let path = unit_variant_path(&uniform.variant, role);
        let variant = UnitVariant::read(&self.vfs.read(&path).map_err(|e| format!("{path}: {e}"))?)
            .map_err(|e| format!("{path}: {e}"))?;
        let (mut model, problems) = SoldierModel::assemble(&self.vfs, &variant, &self.equipment, seed);
        if let Some(theme) = theme.and_then(|t| self.themes.theme(t)).cloned() {
            let mut more = model.equip_from_theme(&self.vfs, &self.themes, &theme, &self.equipment, display, seed);
            more.retain(|p| !p.is_empty());
            for p in more {
                debug!("{unit}: {p}");
            }
        }
        for p in &problems {
            debug!("{unit}: {p}");
        }
        let mut parts = Vec::new();
        for part in model.parts {
            let mesh = assets.meshes.add(empty_mesh(&part));
            let texture = self.part_texture(&part, colours.as_ref(), assets.images);
            let unskinned = matches!(part.binding, PartBinding::Attachment { .. });
            let material = assets.materials.add(StandardMaterial {
                base_color_texture: texture.clone(),
                base_color: if texture.is_some() { Color::WHITE } else { Color::srgb(0.5, 0.5, 0.5) },
                perceptual_roughness: 0.8,
                reflectance: 0.15,
                alpha_mode: AlphaMode::Mask(0.5),
                double_sided: unskinned,
                cull_mode: if unskinned { None } else { Some(Face::Back) },
                ..default()
            });
            parts.push(KitPart { part: Arc::new(part), mesh, material });
        }
        Ok(SoldierKit { parts, posed: None })
    }

    /// The part's diffuse texture, tinted with the uniform colours through its colour mask.
    fn part_texture(
        &mut self,
        part: &SoldierPart,
        colours: Option<&UniformColours>,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        let diffuse = part.textures.diffuse.as_ref()?;
        let mask = part.textures.colour_mask.as_ref().filter(|_| colours.is_some());
        let key = format!("{diffuse}|{mask:?}|{colours:?}");
        if let Some(h) = self.images.get(&key) {
            return Some(h.clone());
        }
        let image = match load_tinted(&self.vfs, diffuse, mask.map(String::as_str), colours) {
            Ok(i) => i,
            Err(e) => {
                warn!("{diffuse}: {e}");
                return None;
            }
        };
        let h = images.add(image);
        self.images.insert(key, h.clone());
        Some(h)
    }
}

/// Removes the clip's forward drift from its root bones so a looping walk stays in place
/// (the unit's own movement carries the man). Bones without a parent are the roots.
pub(crate) fn in_place(mut anim: Anim) -> Anim {
    let n = anim.frames.len();
    if n < 2 {
        return anim;
    }
    let roots: Vec<usize> = (0..anim.bones.len()).filter(|&b| anim.bones[b].parent.is_none()).collect();
    let hips = anim.bone_index("Hips").unwrap_or(0);
    let start = anim.frames[0][hips].translation;
    let end = anim.frames[n - 1][hips].translation;
    for (f, keys) in anim.frames.iter_mut().enumerate() {
        let t = f as f32 / (n - 1) as f32;
        let (dx, dz) = ((end[0] - start[0]) * t, (end[2] - start[2]) * t);
        for &r in &roots {
            keys[r].translation[0] -= dx;
            keys[r].translation[2] -= dz;
        }
    }
    anim
}

fn empty_mesh(part: &SoldierPart) -> Mesh {
    empty_mesh_from(part.vertices.iter().map(|v| v.uv).collect(), part.indices.iter().map(|&i| u32::from(i)))
}

/// A mesh with UVs and triangles, positions/normals filled in later by posing.
fn empty_mesh_from(uvs: Vec<[f32; 2]>, indices: impl Iterator<Item = u32>) -> Mesh {
    let n = uvs.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    // Mirroring Z flips handedness, so swap two indices of every triangle.
    let all: Vec<u32> = indices.collect();
    let mut idx: Vec<u32> = Vec::with_capacity(all.len());
    for t in all.as_chunks::<3>().0 {
        idx.extend([t[0], t[2], t[1]]);
    }
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

fn write_posed(mesh: &Handle<Mesh>, positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, meshes: &mut Assets<Mesh>) {
    if let Some(mut mesh) = meshes.get_mut(mesh) {
        let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.into_iter().map(flip).collect::<Vec<_>>());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.into_iter().map(flip).collect::<Vec<_>>());
    }
}

impl MountKit {
    /// Poses every piece with the mount clip at `frame`. Skips work if already posed there.
    pub fn pose(&mut self, anim: &Anim, clip_id: usize, frame: usize, meshes: &mut Assets<Mesh>) {
        if self.posed == Some((clip_id, frame)) {
            return;
        }
        self.posed = Some((clip_id, frame));
        let bones = anim.world_matrices(frame);
        for (piece, mesh) in &self.pieces {
            let posed = piece.pose(&bones);
            write_posed(mesh, posed.positions, posed.normals, meshes);
        }
    }

    pub fn spawn_parts(&self, commands: &mut Commands, parent: Entity) {
        for (_, mesh) in &self.pieces {
            let child = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(self.material.clone()))).id();
            commands.entity(parent).add_child(child);
        }
    }
}

impl FigureKit {
    /// The clips for `gait`, falling back to the stand clips (then any).
    pub fn anims(&self, gait: Gait) -> Option<&GaitAnims> {
        let find = |g: Gait| self.gaits.iter().find(|(x, _)| *x == g).map(|(_, a)| a);
        find(gait).or_else(|| find(Gait::Stand)).or_else(|| self.gaits.first().map(|(_, a)| a))
    }

    /// The levels for `gait`, falling back to the stand levels (then any).
    pub fn gait_levels(&self, gait: Gait) -> Option<&[KitLevel]> {
        let find = |g: Gait| self.levels.iter().find(|(x, _)| *x == g).map(|(_, l)| l.as_slice());
        find(gait).or_else(|| find(Gait::Stand)).or_else(|| self.levels.first().map(|(_, l)| l.as_slice()))
    }

    /// Poses the man and the mount for `gait` at `time` seconds (looping). The rider and
    /// mount clips are paired (same frame count), so they share the frame index.
    pub fn pose(&mut self, gait: Gait, time: f32, meshes: &mut Assets<Mesh>) {
        let Some(anims) = self.anims(gait).cloned() else { return };
        let id = Gait::ALL.iter().position(|g| *g == gait).unwrap_or(0);
        let frame = frame_at(&anims.man, time);
        self.man.pose(&anims.man, id, frame, meshes);
        if let (Some(kit), Some(anim)) = (self.mount.as_mut(), anims.mount.as_ref()) {
            kit.pose(anim, id, frame.min(anim.frames.len().saturating_sub(1)), meshes);
        }
    }

    /// Spawns the man's and the mount's meshes as children of `parent` (one origin: the
    /// rider clips are authored in the mount's space).
    pub fn spawn_parts(&self, commands: &mut Commands, parent: Entity) {
        self.man.spawn_parts(commands, parent);
        if let Some(m) = &self.mount {
            m.spawn_parts(commands, parent);
        }
    }
}

impl SoldierKit {
    /// Poses every part with `anim` at `frame` (clamped). Skips work if already posed there.
    pub fn pose(&mut self, anim: &Anim, clip_id: usize, frame: usize, meshes: &mut Assets<Mesh>) {
        if self.posed == Some((clip_id, frame)) {
            return;
        }
        self.posed = Some((clip_id, frame));
        let bones = anim.world_matrices(frame);
        for kp in &self.parts {
            let posed = kp.part.pose(&bones);
            write_posed(&kp.mesh, posed.positions, posed.normals, meshes);
        }
    }

    /// Spawns the parts as children of `parent`.
    pub fn spawn_parts(&self, commands: &mut Commands, parent: Entity) {
        for kp in &self.parts {
            let child = commands.spawn((Mesh3d(kp.mesh.clone()), MeshMaterial3d(kp.material.clone()))).id();
            commands.entity(parent).add_child(child);
        }
    }
}

/// Decodes a DDS (all mips) to an sRGB image; with a colour mask and colours, tints level 0
/// and rebuilds the mips by 2x2 averaging.
pub fn load_tinted(
    vfs: &Vfs,
    diffuse: &str,
    mask: Option<&str>,
    colours: Option<&UniformColours>,
) -> Result<Image, String> {
    let bytes = vfs.read(diffuse).map_err(|e| e.to_string())?;
    let dds = Dds::parse(&bytes).map_err(|e| e.to_string())?;
    let (w, h) = (dds.width, dds.height);
    let mut base = dds.decode_rgba8(0);
    let mut levels = dds.mip_count.max(1);
    if let (Some(mask), Some(colours)) = (mask, colours) {
        let mbytes = vfs.read(mask).map_err(|e| e.to_string())?;
        let mdds = Dds::parse(&mbytes).map_err(|e| e.to_string())?;
        let (mw, mh) = (mdds.width, mdds.height);
        let m = mdds.decode_rgba8(0);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (x * mw / w.max(1), y * mh / h.max(1));
                let mi = ((sy * mw + sx) * 4) as usize;
                let di = ((y * w + x) * 4) as usize;
                let t = tint(
                    [base[di], base[di + 1], base[di + 2], base[di + 3]],
                    [m[mi], m[mi + 1], m[mi + 2], m[mi + 3]],
                    colours,
                );
                base[di..di + 4].copy_from_slice(&t);
            }
        }
        // Rebuild mips from the tinted level 0 (power-of-two sizes only; else one level).
        levels = if w.is_power_of_two() && h.is_power_of_two() { levels } else { 1 };
    }
    let mut data = base.clone();
    let (mut lw, mut lh, mut prev) = (w, h, base.clone());
    for level in 1..levels {
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let next = if mask.is_some() && colours.is_some() {
            let mut out = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let mut s = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let (px, py) = ((x * 2 + dx).min(lw - 1), (y * 2 + dy).min(lh - 1));
                            s += u32::from(prev[((py * lw + px) * 4 + c) as usize]);
                        }
                        out[((y * nw + x) * 4 + c) as usize] = (s / 4) as u8;
                    }
                }
            }
            out
        } else {
            dds.decode_rgba8(level)
        };
        data.extend_from_slice(&next);
        (lw, lh, prev) = (nw, nh, next);
    }
    let size = Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, base, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    Ok(image)
}

/// Which clip frame to show at `time` seconds (looping, the clip's own frame rate).
pub fn frame_at(anim: &Anim, time: f32) -> usize {
    let n = anim.frames.len().max(1);
    let rate = if anim.frame_rate > 0.0 { anim.frame_rate } else { 20.0 };
    ((time * rate) as usize) % n
}
