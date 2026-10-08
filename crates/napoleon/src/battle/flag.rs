//! The standard bearer's flag, drawn: the verlet cloth of
//! `RigidModels\VerletItems\standard_bearer_flag.logic` hanging from the bearer's pole.
//!
//! The **solve** is `ntw_sim::battle::cloth` and the **reader** is `ntw_formats::cloth` (which
//! decodes the file into the plain arrays the solve takes); this module only
//!
//! 1. reads the `.logic` and the flag texture atlas when the battle starts,
//! 2. gives every unit that has a standard bearer one cloth entity, and
//! 3. steps each cloth from its bearer's pole bone and writes the result into its mesh.
//!
//! **Where the attachment comes from** (all CONFIRMED, `UNITS_TERRAIN_FIDELITY.md` §3):
//! `variantmodels\equipment\mesh.variant_part_mesh` binds `rigid_equip_euro_flagpole_lod1/_lod2`
//! to **bone 3**; in the FLAG_BEARER skeleton (`Animations/MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim`)
//! bone 3 is `Weapon3`, whose frame-0 **+x axis is world up**. So the pole is bone 3's frame, and
//! the sail's hoist edge is bone-local x `1.027 .. 2.183`, i.e. **1.43 m to 2.58 m above the
//! bearer's feet** ([`ntw_formats::cloth::bone3_at_ground`]).
//!
//! **INFERRED** (review 0-D): the verlet item is drawn shifted by `-FRAME_OFFSET` from the pole
//! bone ([`ntw_formats::cloth::verlet_frame`]) so its mast lies on the drawn pole; the file's own
//! coordinates put the mast 0.967 m off the pole's axis, and the exe's transform is UNKNOWN.
//! `0x01227BD0` below and `0x00732190` are exe readings whose decompiles are not kept in the repo
//! or the sandbox evidence, so what they are said to do is INFERRED, not CONFIRMED.
//!
//! **PROVISIONAL, each with its exact target:**
//!
//! - The wind is the battle file's `weather/prevailing_wind`, read as metres per second
//!   (Austerlitz ships `(0, 9)`; five shipped battle files ship `(0, 0)`, and their flags hang).
//!   Target: the exe-side wind speed the verlet item is fed. INFERRED, not CONFIRMED.
//! - `ITERATIONS`, `SUBSTEPS` and `DAMPING` in [`ntw_sim::battle::cloth`]. Target: the exe-side
//!   per-frame verlet update reached from `0x00732190`.
//! - **The flag key is the last segment of `factions.flag_path`**, not the faction key (INFERRED:
//!   the data singles it out, the exe's lookup is not read). CONFIRMED from the data over all 77
//!   factions: that segment is a key of
//!   `rigidmodels\flags\textures\flags.tai` for **every** one, and it is the **only** column that
//!   resolves all **39** *dependent* factions (`spa_france`, `egy_ottomans`, `*_rebels`, `tut_*`)
//!   that have no `flag_<faction>.tga` of their own — `model_faction` resolves 22 of them,
//!   `rebel_flag_path` 26, `republic_flag_path` 10, `faction_group` 7, `subculture` none (install
//!   test `flag_faction_install::only_flag_path_resolves_every_dependent_faction`).
//!   **PROVISIONAL on one point, and round 10 moved the target:** `0x01227BD0` completing `flag_`
//!   and falling back to `flag_default.tga` is an exe reading (decompile not kept, INFERRED), and that function is the flag system's
//!   **registry lookup** — it has **one** caller and that caller hands it the literal `"default"`,
//!   so it is *not* what a faction's flag is looked up by. The name is a record field at `+0x38`
//!   of a table this pass did not identify, and everything around that function is **campaign-map**
//!   flag art (`campaignflag.fx`, `Flag_primary`, `naval_ID_frame`, `naval_id_`, `flag_orn_*`, the
//!   `army_flag_scale` / `occupied_settlement_flag_scale` CVars), so whether the standard bearer's
//!   cloth reads this atlas at all is **UNKNOWN**. Target: the writer of the flag record array.
//!   See [`FlagLibrary::flag_of`] and `UNITS_TERRAIN_FIDELITY.md` §3.2a.
//! - The sail is two-sided, unlit by a normal map and casts no shadow. Target: the exe's flag
//!   material and its shadow pass.
//! - Where in the formation the bearer stands (see [`super::view`]). Target: the exe's standard
//!   bearer placement in the unit's formation.

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use ntw_formats::cloth::{self, standard_bearer_flag};
use ntw_formats::pack::Vfs;
use ntw_formats::texture_atlas::TaiAtlas;
use ntw_sim::battle::cloth::FlagCloth;

use super::view::{PoleBone, UnitView};
use super::BattleSim;

/// The shipped flag cloth file and the atlas that holds the flag images.
const LOGIC: &str = r"rigidmodels\verletitems\standard_bearer_flag.logic";
const ATLAS: &str = r"rigidmodels\flags\textures\flags.tai";
const ATLAS_DIR: &str = r"rigidmodels\flags\textures";

/// Everything the flags of a battle share, read once when it starts.
#[derive(Resource, Default)]
pub struct FlagLibrary {
    /// The shipped flag cloth, as authored. Every unit gets its own copy of it.
    pub template: Option<FlagCloth>,
    /// `flag_<key>.tga` -> (atlas page, the image's `A` corner, its `B` corner).
    rects: BTreeMap<String, (usize, [f32; 2], [f32; 2])>,
    /// One image per atlas page, in the order the `.tai` first names them.
    pages: Vec<Handle<Image>>,
}

impl FlagLibrary {
    /// The atlas rectangle of a faction's flag.
    ///
    /// The flag a faction flies is **the last segment of `factions.flag_path`**
    /// (`data\ui\flags\france` -> `france`), and it is looked up as `flag_<that>.tga`.
    ///
    /// **CONFIRMED from the data, over all 77 factions:** that segment is a key of `flags.tai` for
    /// **every** faction -- and for the **39** dependent ones (`egy_*`, `ita_*`, `spa_*`, `tut_*`,
    /// `*_rebels`) that have **no `flag_<faction>.tga` of their own it is the *only* column that
    /// resolves all of them**. `factions.faction_group` resolves 7, `republic_flag_path` 10,
    /// `model_faction` 22, `rebel_flag_path` 26 and `subculture` none, and the ones that do resolve
    /// *disagree* (`french_rebels` would fly `france` under `model_faction`). Install tests
    /// `flag_faction_install::every_factions_flag_key_is_in_the_shipped_atlas` and
    /// `flag_faction_install::only_flag_path_resolves_every_dependent_faction`.
    ///
    /// **PROVISIONAL**, with its target named, and round 10 moved that target: the key-first-then-
    /// default read is INFERRED from an exe reading whose decompile is not kept (`0x01227BD0`
    /// completes `flag_` with whatever string it is given and falls back to `flag_default.tga`), and `0x01227BD0` is the flag system's **registry
    /// lookup** -- one caller, and that caller hands it the literal `"default"` -- so it is not what
    /// a faction's flag is looked up by, and everything around it is **campaign-map** flag art.
    /// What names a flag record is a field at `+0x38` of a table this pass did not identify. This
    /// order (own image, then `flag_path`, then `republic_flag_path`, then the default) is the one
    /// the data supports. `UNITS_TERRAIN_FIDELITY.md` §3.2a has the evidence.
    pub fn flag_of(&self, db: &crate::data::GameData, faction: &str) -> Option<(Handle<Image>, [f32; 2], [f32; 2])> {
        let row = db.db.factions.get(faction);
        let stem = |p: &str| p.rsplit(['\\', '/']).next().unwrap_or(p).to_owned();
        let mut candidates: Vec<String> = vec![format!("flag_{faction}.tga")];
        for key in row.map(|r| stem(&r.flag_path)).into_iter().chain(row.and_then(|r| r.republic_flag_path.as_deref().map(stem))) {
            let name = format!("flag_{key}.tga");
            if !candidates.contains(&name) {
                candidates.push(name);
            }
        }
        candidates.push("flag_default.tga".to_owned());
        let found = candidates.iter().find_map(|name| self.rects.get(name))?;
        let page = self.pages.get(found.0)?.clone();
        Some((page, found.1, found.2))
    }
}

/// One unit's drawn flag. The entity's own `Transform` is the identity: the cloth mesh carries
/// **world** positions, so it inherits nothing.
#[derive(Component)]
pub struct FlagView {
    /// The model unit whose standard bearer carries it.
    pub unit: u32,
    /// The solve, written in place every frame.
    cloth: FlagCloth,
    /// The Bevy mesh whose positions and normals are rewritten every frame.
    mesh: Handle<Mesh>,
    /// The bone matrix the cloth was last stepped against, so a teleport resets it instead of
    /// dragging the cloth across the map.
    last_bone: Option<[f32; 16]>,
}

/// Reads the flag cloth and its texture atlas when the battle starts. Nothing is drawn when the
/// install cannot be read; the battle then falls back to unit outlines, as it does without the
/// soldier library.
pub fn load(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut lib = FlagLibrary::default();
    let dir = crate::config::game_data_dir();
    let vfs = match Vfs::open_install(&dir) {
        Ok(v) => v,
        Err(e) => {
            warn!("Flag cloth unavailable ({e})");
            return;
        }
    };
    match vfs.read(LOGIC) {
        Ok(bytes) => match String::from_utf8(bytes).map_err(|e| e.to_string()) {
            Ok(text) => match standard_bearer_flag(&text) {
                // The reader hands the solve plain arrays; nothing here decodes or solves.
                Some(spec) => lib.template = Some(FlagCloth::new(spec.parts())),
                None => warn!("Flag cloth unavailable ({LOGIC}): no standard-bearer item in it"),
            },
            Err(e) => warn!("Flag cloth unavailable ({LOGIC}): {e}"),
        },
        Err(e) => warn!("Flag cloth unavailable ({LOGIC}): {e}"),
    }
    match vfs.read(ATLAS) {
        Ok(bytes) => match String::from_utf8(bytes).map_err(|e| e.to_string()).and_then(|t| TaiAtlas::read(&t).map_err(|e| e.to_string())) {
            Ok(atlas) => {
                for page in &atlas.pages {
                    let path = format!("{ATLAS_DIR}\\{page}");
                    match crate::soldiers::load_tinted(&vfs, &path, None, None) {
                        Ok(image) => lib.pages.push(images.add(image)),
                        Err(e) => warn!("Flag atlas page {path}: {e}"),
                    }
                }
                for (name, e) in &atlas.entries {
                    let page = atlas.pages.iter().position(|p| p == &e.page).unwrap_or(0);
                    lib.rects.insert(name.clone(), (page, e.a, e.b));
                }
            }
            Err(e) => warn!("Flag atlas unavailable ({ATLAS}): {e}"),
        },
        Err(e) => warn!("Flag atlas unavailable ({ATLAS}): {e}"),
    }
    if let Some(c) = lib.template.as_ref() {
        info!(
            "Flag cloth: {} particles, {} triangles, {} atlas pages, {} flags, {} ropes, {} pins",
            c.particles(),
            c.triangles.len(),
            lib.pages.len(),
            lib.rects.len(),
            c.links.len(),
            c.pinned().count()
        );
    }
    commands.insert_resource(lib);
}

/// Gives every unit with a standard bearer its flag: one cloth entity in world space, with that
/// unit's faction's flag mapped into the mesh's texture coordinates once.
pub fn spawn_flags(
    mut commands: Commands,
    sim: Res<BattleSim>,
    data: Res<crate::data::GameData>,
    lib: Option<Res<FlagLibrary>>,
    drawn: Query<&FlagView>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only when the battle changed, and only once: the meshes are written by `sync_flags`.
    if !sim.is_changed() || !drawn.is_empty() {
        return;
    }
    let Some(lib) = lib.as_ref() else { return };
    let Some(template) = lib.template.as_ref() else { return };
    let mut made = 0usize;
    for info in &sim.info {
        // Only a unit that ships a standard bearer carries a flag: `unit_stats_land` #4/5/6
        // officer / musician / standard bearer feed `battle_personalities` (CONFIRMED, read in
        // `ntw_formats::unit_animation::plan_figure`).
        let Some(stats) = data.db.unit_stats(&info.key) else { continue };
        if stats.standard_bearer.as_deref().is_none_or(str::is_empty) {
            continue;
        }
        let Some((image, a, b)) = lib.flag_of(&data, &info.faction) else { continue };
        // MAIN_WORLD too: `sync_flags` rewrites the positions every frame, and Bevy refuses to edit
        // a render-world-only mesh once it has been extracted (it panicked on the user's install).
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        // The cloth's own `t(u, v)` is the flag image's own 0..1; the atlas entry says where that
        // image sits in the page, so the drawn coordinate is `A + t * (B - A)` (CONFIRMED: the
        // `.tai` header defines A and B as the image's two corners).
        let uv: Vec<[f32; 2]> = template
            .uv
            .iter()
            .map(|t| [a[0] + t[0] * (b[0] - a[0]), a[1] + t[1] * (b[1] - a[1])])
            .collect();
        // The authored shape at the world origin, only so the mesh is complete; the flag stays
        // hidden until `sync_flags` has placed it on its bearer's pole.
        let mut placed = template.clone();
        placed.reset(&cloth::verlet_frame(&cloth::bone3_at_ground()));
        write_cloth(&mut mesh, placed.positions(), &placed.normals(), &uv, &template.triangles);
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(image),
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            // Two-sided: the back face is lit with its own (flipped) normal.
            double_sided: true,
            perceptual_roughness: 1.0,
            ..default()
        });
        let handle = meshes.add(mesh);
        commands.spawn((
            Mesh3d(handle.clone()),
            MeshMaterial3d(material),
            Visibility::Hidden,
            // The vertices are rewritten in world space every frame, so the bounds computed from
            // the first write would go stale and cull the flag.
            bevy::camera::visibility::NoFrustumCulling,
            FlagView { unit: info.id, cloth: template.clone(), mesh: handle, last_bone: None },
        ));
        made += 1;
    }
    if made > 0 {
        info!("{made} units carry a standard bearer's flag");
    }
}

/// Steps every drawn flag from its bearer's pole bone and rewrites its mesh. Runs after
/// `view::sync_views`, which is what fills in each bearer's [`PoleBone`].
pub fn sync_flags(
    sim: Res<BattleSim>,
    time: Res<Time>,
    bearers: Query<(&ChildOf, &GlobalTransform, &PoleBone, &InheritedVisibility)>,
    units: Query<&UnitView>,
    mut flags: Query<(&mut FlagView, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if flags.is_empty() || bearers.is_empty() {
        return;
    }
    let wind = sim.wind;
    let dt = time.delta_secs();
    for (mut f, mut vis) in &mut flags {
        // The bearer is a **child** of his unit's view entity (`view::spawn_missing_views`): the
        // `PoleBone` sits on the bearer, the `UnitView` on the parent. (Round 8 queried both on one
        // entity, which matched nothing, so no flag was ever stepped or moved off the origin.)
        let owner = bearers
            .iter()
            .find(|(parent, ..)| units.get(parent.parent()).is_ok_and(|u| u.id == f.unit))
            .map(|(_, xform, pole, shown)| (xform, pole, shown.get()));
        let Some((bearer_xform, pole, shown)) = owner else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        // The flag goes where its bearer goes: hidden with him (a routed or dead unit).
        vis.set_if_neq(if shown { Visibility::Visible } else { Visibility::Hidden });
        // The figure is drawn as `bearer transform * mirror(bone * vertex)` (the skinning shader
        // negates z after the bone transform), so the pole is at `bearer * S * bone`, and the
        // verlet item sits on it through `verlet_frame` (INFERRED, see `ntw_formats::cloth`).
        let bone = ntw_formats::anim::mat_mul(
            &bearer_xform.to_matrix().to_cols_array(),
            &cloth::verlet_frame(&cloth::mirror_z(pole.model)),
        );
        // A jump of more than half a metre in one frame is a teleport, not a stride: put the cloth
        // back on the mast rather than dragging it across the map.
        let jumped = f.last_bone.is_none_or(|last| (0..16).map(|k| (last[k] - bone[k]).powi(2)).sum::<f32>() > 0.25);
        if jumped {
            f.cloth.reset(&bone);
        }
        f.cloth.step(dt, wind, &bone);
        f.last_bone = Some(bone);
        let Some(mut mesh) = meshes.get_mut(&f.mesh) else { continue };
        // Positions and normals only: the texture coordinates are the atlas-mapped ones
        // `spawn_flags` wrote, and must not be replaced by the cloth's raw `t(u, v)` (round 8 did,
        // which would have drawn the whole atlas page on every flag after the first frame).
        write_motion(&mut mesh, f.cloth.positions(), &f.cloth.normals());
    }
}

/// Writes positions, normals, texture coordinates and indices into a Bevy mesh.
///
/// The cloth is solved in **Bevy world space** (its bone matrix already carries the figures' z
/// mirror, see `sync_flags`), so nothing is flipped here and the triangles keep the file's own
/// order; the normals come from those same triangles, so they agree with the winding, and the
/// material is double-sided.
fn write_cloth(mesh: &mut Mesh, positions: &[[f32; 3]], normals: &[[f32; 3]], uv: &[[f32; 2]], triangles: &[[u32; 3]]) {
    if positions.len() != uv.len() {
        return;
    }
    write_motion(mesh, positions, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.to_vec());
    mesh.insert_indices(Indices::U32(triangles.iter().flatten().copied().collect()));
}

/// The per-frame part of [`write_cloth`]: the live positions and their normals.
fn write_motion(mesh: &mut Mesh, positions: &[[f32; 3]], normals: &[[f32; 3]]) {
    if positions.len() != normals.len() {
        return;
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.to_vec());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.to_vec());
}