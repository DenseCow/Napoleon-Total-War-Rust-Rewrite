//! # ntw_formats
//!
//! Byte-level readers for the file formats of *Napoleon: Total War* (2010).
//!
//! The game reads the player's **own installed copy** of the original files.
//! Everything here opens files read-only. No API in this crate can write to a
//! `.pack` file or an install folder. The only writer, [`esf::EsfWriter`],
//! produces bytes in memory.
//!
//! | module | format | used for |
//! |---|---|---|
//! | [`esf`] | ESF "ABCE" binary tree | startpos, saves, campaign map data |
//! | [`pack`] | `.pack` "PFH0" archives + [`pack::Vfs`] | every packed game file |
//! | [`db`] | binary DB tables (`db\<name>_tables\<name>`) | units, buildings, rules, ... |
//! | [`loc`] | `.loc` localisation tables | all on-screen text |
//! | [`mount`] | `mount_variants` -> `warscape_animated[_lod]` -> horse meshes | cavalry mounts |
//! | [`dds`] | `.dds` textures (DXT1/3/5, uncompressed) + CPU decoder | every model texture |
//! | [`rigid_model`] | `.rigid_model` static meshes + `.rigid_model_header` | buildings, props, campaign pieces |
//! | [`battle_terrain`] | `battleterrain\presets\<map>\` (L16 heightfields, XML, ESF lists) | battle maps |
//! | [`battle_markers`] | battle-map `.markers` (prop and polygon markers) | battlefield props, clear areas |
//! | [`campaign_map`] | `campaign_maps\<map>\` heightmap, supertexture, SPLN splines, `regions.esf` | the campaign map |
//! | [`campaign_pathfinding`] | `pathfinding.esf` (with its value cipher), `sea_grids.esf` | campaign movement, AI sea zones |
//! | [`tga`] | `.tga` images (palette, true colour, RLE) | UI art, ground-type maps, terrain masks |
//! | [`xml`] | small XML settings files | battle-map metadata |
//! | [`unit_variant`] | `.unit_variant` part lists + `.variant_part_mesh` LOD streams | soldiers and equipment |
//! | [`anim`] | `.anim` skeletal clips (skeleton + per-frame bone keys) | soldier and animal animation |
//! | [`battle_animation`] | `animation_tables.txt` + `battleconfiguration\*_fragment.txt` | which clip plays for each action |
//! | [`unit_animation`] | unit_stats_land + battle_personalities + animation tables | figures, mounts, clips per gait |
//! | [`unit_model`] | unit key -> uniforms -> variant parts, equipment, textures, colours; CPU skinning | soldiers on screen |
//! | [`weighted_mesh`] | `.variant_weighted_mesh` skinned whole-body meshes | horses, camels, campaign agents |
//! | [`ui_layout`] | binary UI layouts (`"VersionNNN"`) | menus, HUD, every screen |
//! | [`font`] | `.cuf` bitmap fonts + `fontcategories.fc` | all UI text |
//! | [`vegetation`] | `warscape_trees` -> `.spt`, `data.tree_model`, `*_compositemap.txt` | battle trees |
//! | [`effects`] | `effects\landbattle.xml` + `effects\unit_dust_parameters.txt` | battle particles: smoke, muzzle flashes, dust |
//! | [`projectile_fx`] | `db\projectiles_explosions`, `db\projectile_impacts` | the groups an explosion and an impact play |
//! | [`verlet`] | `RigidModels\VerletItems\*.logic` | the standard bearer's flag cloth |
//! | [`cloth`] | a `VerletItems\*.logic` item as the plain arrays `ntw_sim::battle::cloth` solves | the flag's shape |
//! | [`texture_atlas`] | `*.tai` texture-atlas lists (plain text) | the flag atlas |
//! | [`sound`] | `sounds_packed\sound_events`, `.mp3`/`.wav` decoding | music, effects, voices |
//! | [`bink`] | Bink 1 `.bik` movies (our own decoder; `binkw32.dll` is never used) | intro, front-end and campaign movies |
//!
//! All formats are little-endian. The specs come from the project's research
//! reports (see each module's docs).

pub mod battle_spec;
pub mod battle_markers;
pub mod bink;
pub mod battle_terrain;
pub mod campaign_map;
pub mod campaign_pathfinding;
pub mod cloth;
mod bytes;
pub mod anim;
pub mod battle_animation;
pub mod db;
pub mod dds;
pub mod esf;
pub mod effects;
pub mod font;
pub mod group_formation;
pub mod loc;
pub mod models_building;
pub mod mount;
pub mod pack;
pub mod preferences;
pub mod projectile_fx;
pub mod rigid_model;
pub mod sound;
pub mod speedtree;
pub mod tga;
pub mod texture_atlas;
pub mod vegetation;
pub mod xml;
pub mod unit_animation;
pub mod unit_model;
pub mod unit_variant;
pub mod verlet;
pub mod weighted_mesh;
pub mod ui_layout;
pub mod ui_templates;
