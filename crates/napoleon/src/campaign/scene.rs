//! Builds the campaign scene on entering `GameMode::Campaign`: the model (startpos + DB rules +
//! movement grid), the scripts, terrain, lines, settlements, marker assets, camera and light.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat};
use ntw_formats::campaign_map::{CampaignMap, DISPLAY_TO_LOGIC, GameFiles, HEIGHT_SCALE};
use ntw_formats::pack::Vfs;
use ntw_formats::rigid_model::RigidModel;
use ntw_script::{ScriptContext, ScriptHost, ScriptSource};
use ntw_sim::campaign::{CampaignModel, CharacterId, CharacterKind, FactionId, RegionId, Terrain};

use super::CampaignStart;
use super::camera::{self, CampaignCamera};
use super::play::{CampaignSim, CharacterMarker, SettlementFlag};
use crate::GameMode;
use crate::data::GameData;
use crate::model_viewer::load_dds;
use crate::model_viewer::source::{ModelSource, NameFlags, convert_mesh};

use ntw_data::campaign::SLOT_TYPE_SETTLEMENT;

/// The loaded map, for height queries (camera, markers).
#[derive(Resource, Clone)]
pub struct CampaignGround(pub Arc<CampaignMap>);

/// The settlement **fortification** meshes, and the one settlement slot's walls are currently drawn
/// with.
///
/// A settlement's walls are not a separate map object: they are the settlement's own template model
/// with `_slot_fortifications_lvl<level>` appended. The files exist under exactly those names for
/// every culture but `NA` (CONFIRMED, install test `slots_install`). That the exe's model-name
/// builder `0x00B42B90` is what appends it (the literal at `0x0137D604`, one reference) is an exe
/// reading whose decompile is not kept in the repo or the sandbox evidence: INFERRED. The name comes
/// from the shipped `slots_art` -> `slots_templates_models` chain, so nothing is spelled by hand.
///
/// **INFERRED: the file level is the `sFortifications` chain level + 1.** The data gives three files
/// (`_lvl0 < _lvl1 < _lvl2` in vertex count for every culture and slot number, install test
/// `slots_install::the_settlement_fortification_levels_are_a_rising_ladder`) and two chain levels
/// (0 and 1); `fFort`'s own files are `fort_lvl<level + 1>`. That fits "+1" but does not prove it
/// -- a vertex ladder says nothing about which building level asks for which file.
///
/// **Open, and deliberately not drawn:** under the "+1" reading `_lvl0` is the unfortified mesh,
/// and the exe would then draw it on every settlement without walls (which is all of them at the
/// start of every shipped campaign). We draw nothing for an unfortified settlement, because that
/// consequence rests on two inferences (the offset, and additive drawing) and would change every
/// settlement on the map. Target: `0x00B42B90`'s caller, with the decompile kept.
///
/// PROVISIONAL, and the one thing to look at in game: the fortification mesh is drawn **in
/// addition to** the settlement's own `<stem>_<n>_slot.rigid_model`, not instead of it (INFERRED:
/// the Ottoman `_lvl1`/`_lvl2` meshes do not overlap the plain mesh's footprint at all, and the
/// names keep `_slot_` as a qualifier of the same slot).
#[derive(Resource, Default)]
pub struct SettlementWalls {
    /// One `(folder, stem)` per culture row of `slots_art`'s `settlement` slot type, in the
    /// table's own order. Copied out of the database because [`spawn_static`] holds `&mut World`.
    templates: Vec<(String, String)>,
    /// The `sFortifications` chain's `building_levels` rows: level key -> chain level. Only the
    /// two shipped rows, copied so the level needs no database at draw time.
    levels: HashMap<String, i32>,
    /// `(slot number, file level)` -> the loaded mesh parts, shared by every settlement that uses
    /// them (238 settlements share about twenty models).
    parts: HashMap<(u32, u32), Parts>,
}

/// One settlement's walls, swapped when its fortification is built, improved or demolished
/// (`sFortifications` has two levels; the file level is the chain level + 1, so a settlement with
/// no fortification building draws nothing).
#[derive(Component)]
pub struct SettlementWallsView {
    /// The region whose `REGION_SLOT_MANAGER/FORTIFICATION_SLOT` building decides the level.
    pub region: RegionId,
    /// The `settlement_<n>_slot` number (1..=5 on the shipped maps).
    pub slot: u32,
    /// The file level currently drawn (`None` = nothing drawn).
    pub level: Option<u32>,
    /// The child entities holding the mesh parts, so a level change can replace them.
    parts: Vec<Entity>,
}

impl SettlementWalls {
    /// Copies the settlement templates out of the database (`slots_art`'s `settlement` rows, each
    /// through `slots_templates_models`) and the `sFortifications` chain levels out of
    /// `building_levels`.
    pub fn from_db(db: &ntw_data::GameDatabase) -> Self {
        let mut templates = Vec::new();
        for row in db.campaign.slot_art.iter().filter(|r| r.slot_type == SLOT_TYPE_SETTLEMENT) {
            let Some(key) = row.template.as_deref() else { continue };
            let Some(t) = db.campaign.slot_template_model(key) else { continue };
            templates.push((t.folder_path(), t.model.to_ascii_lowercase()));
        }
        let levels = db
            .building_levels
            .iter()
            .filter(|l| l.chain == SETTLEMENT_FORTIFICATION_CHAIN)
            .map(|l| (l.key.clone(), l.level))
            .collect();
        Self { templates, levels, parts: HashMap::new() }
    }

    /// The file level a settlement's fortification building calls for: the `sFortifications` chain
    /// level **+ 1** (INFERRED, see [`SettlementWalls`]). `None` when the region has no
    /// fortification building (or one this table has no level for), which draws nothing -- the
    /// `_lvl0` question is open, see [`SettlementWalls`].
    pub fn level_of(&self, level_key: Option<&str>) -> Option<u32> {
        let chain = *self.levels.get(level_key?)?;
        Some(u32::try_from(chain).ok()?.saturating_add(1).max(1))
    }

    /// Every candidate path for `(n, level)`: first the template whose stem is the one the plain
    /// settlement mesh is drawn from ([`SETTLEMENT_CITY_STEM`], so a city and its walls are always
    /// the same culture's art), then the rest in the `slots_art` order. PROVISIONAL in the same way
    /// as the plain mesh: a region's culture is not read from the save. (Round 10 took the first
    /// culture in table order that had the file, which could put another culture's walls round a
    /// European city; fixed in review 0-D.)
    fn candidates(&self, n: u32, level: u32) -> impl Iterator<Item = String> + '_ {
        let own = self.templates.iter().filter(|(_, stem)| stem == SETTLEMENT_CITY_STEM);
        let rest = self.templates.iter().filter(|(_, stem)| stem != SETTLEMENT_CITY_STEM);
        own.chain(rest)
            .map(move |(folder, stem)| format!("{folder}\\{stem}_{n}_slot_fortifications_lvl{level}.rigid_model"))
    }

    /// Loads (once) and returns the parts for `(n, level)`, or `None` when the install ships no
    /// such mesh -- the tribal (`NA`) template never has one, and the port and town templates have
    /// none at all (CONFIRMED over the pack). `load` reads and converts one named model.
    fn get(
        &mut self,
        src: &ModelSource,
        n: u32,
        level: u32,
        mut load: impl FnMut(&str) -> Parts,
    ) -> Option<Parts> {
        if let Some(p) = self.parts.get(&(n, level)) {
            return Some(p.clone());
        }
        let path = self.candidates(n, level).find(|p| src.vfs.contains(p))?;
        let parts = load(&path);
        if !parts.is_empty() {
            self.parts.insert((n, level), parts.clone());
        }
        Some(parts)
    }

    /// The mesh parts currently loaded for `(n, level)`, if any (no loading).
    fn parts_of(&self, n: u32, level: u32) -> Option<&Parts> {
        self.parts.get(&(n, level))
    }
}

/// The building chain a settlement's own walls belong to (`building_chains`).
const SETTLEMENT_FORTIFICATION_CHAIN: &str = "sFortifications";

/// Loads every fortification mesh the install has, for every slot number and both walled levels, so
/// [`sync_walls`] can put walls on a settlement the player builds them on **later in the session**
/// (it has no model source of its own). One model per `(slot number, level)` -- the shipped maps use
/// slot numbers 1..=5, so at most ten models, each read once and shared by every settlement that
/// uses it; the cost is a few `contains` lookups per candidate and ten model reads.
fn warm_walls(
    walls: &mut SettlementWalls,
    src: &ModelSource,
    slots: impl Iterator<Item = u32>,
    cache: &mut ModelCache,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> usize {
    let mut n_models = 0;
    for n in slots {
        for level in 1..=2u32 {
            if walls.get(src, n, level, |p| cache.get(src, p, meshes, materials, images)).is_some() {
                n_models += 1;
            }
        }
    }
    n_models
}

/// Heightmap samples skipped per terrain vertex (4096x2048 -> 1025x513 vertices).
pub(super) const TERRAIN_STRIDE: u32 = 4;
/// The supertexture level drawn: the first one at most this wide (8x4 tiles = 4096x2048 for Europe).
pub(super) const MAX_TEXTURE_WIDTH: u32 = 4096;
/// Lines float this far above the ground (logic units), PLACEHOLDER.
pub(super) const LINE_LIFT: f32 = 0.06;

pub(super) fn bevy_pos(x: f32, h: f32, z: f32) -> Vec3 {
    Vec3::new(x, h, -z)
}

/// Meshes and materials for character markers, built once on entering the mode.
#[derive(Resource)]
pub struct MarkerAssets {
    army: Handle<Mesh>,
    navy: Handle<Mesh>,
    agent: Handle<Mesh>,
    /// Faction primary colour (`factions` table) and its material, by faction id.
    factions: HashMap<FactionId, (Color, Handle<StandardMaterial>)>,
    /// The faction's original ID badge parts (`campaignagenticons\agent_id_<faction>`).
    badges: HashMap<FactionId, Parts>,
    grey: Handle<StandardMaterial>,
}

impl MarkerAssets {
    /// A faction's primary colour (grey if unknown).
    pub fn colour(&self, f: FactionId) -> Color {
        self.factions.get(&f).map_or(Color::srgb(0.5, 0.5, 0.5), |c| c.0)
    }

    /// Spawns one character marker. PLACEHOLDER shapes in the faction colour (army = cone,
    /// navy = box, agent = ball) with the original faction badge floating above (INFERRED use).
    pub fn spawn_marker(&self, commands: &mut Commands, id: CharacterId, faction: FactionId, navy: Option<bool>, kind: CharacterKind, pos: Vec3) {
        let mesh = match (navy, kind) {
            (Some(true), _) => self.navy.clone(),
            (Some(false), _) | (None, CharacterKind::General) => self.army.clone(),
            _ => self.agent.clone(),
        };
        let mat = self.factions.get(&faction).map_or(self.grey.clone(), |c| c.1.clone());
        let badge = self.badges.get(&faction).cloned().unwrap_or_default();
        commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(pos),
                Name::new(format!("character {} {}", id.0, kind.esf_name())),
                CharacterMarker { id },
                DespawnOnExit(GameMode::Campaign),
            ))
            .with_children(|c| {
                for (m, mat) in &badge {
                    c.spawn((Mesh3d(m.clone()), MeshMaterial3d(mat.clone()), Transform::from_xyz(0.0, 1.4, 0.0).with_scale(Vec3::splat(DISPLAY_TO_LOGIC))));
                }
            });
    }
}

/// `OnEnter(GameMode::Campaign)`: loads the start position, the map and the scripts, and draws
/// the static scene. Characters are drawn by `play::sync_markers`.
pub fn enter(world: &mut World) {
    let start = world.get_resource::<CampaignStart>().cloned().unwrap_or_default();
    let dir = crate::config::game_data_dir();
    let vfs = match Vfs::open_install(&dir) {
        Ok(v) => v,
        Err(e) => {
            error!("Campaign: cannot open the install at {}: {e}", dir.display());
            return;
        }
    };
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let startpos = match &start.save {
        Some(p) => p.display().to_string(),
        None => format!("campaigns/{}/startpos.esf", start.campaign),
    };
    let read = match &start.save {
        Some(p) => std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())),
        None => files.read(&startpos),
    };
    let bytes = match read {
        Ok(b) => b,
        Err(e) => {
            error!("Campaign {}: {e}", start.campaign);
            return;
        }
    };
    let mut loaded = match ntw_campaign::read(&bytes, &world.resource::<GameData>().db) {
        Ok(l) => l,
        Err(e) => {
            error!("Campaign {}: {startpos}: {e}", start.campaign);
            return;
        }
    };
    // A save names its own campaign.
    let mut start = start;
    if start.save.is_some() {
        start.campaign = loaded.info.campaign_key.clone();
    }
    info!(
        "Campaign {}: map {}, {} factions, {} regions, {} characters, {} forces ({} warnings)",
        start.campaign,
        loaded.info.map_key,
        loaded.model.world.factions.len(),
        loaded.model.world.regions.len(),
        loaded.model.world.characters.len(),
        loaded.model.world.forces.len(),
        loaded.warnings.len()
    );
    // Each oddity the loader found, once per load.
    for w in &loaded.warnings {
        warn!("Campaign load: {w}");
    }
    // The save header's territory pictures (one per theatre the header has), rebuilt on save.
    let pictures: Vec<ntw_campaign::header_map::TheatrePictures> = loaded
        .info
        .header
        .maps
        .iter()
        .filter_map(|m| ntw_campaign::header_map::TheatrePictures::load(&files, &world.resource::<GameData>().db, &loaded.info.map_key, &m.theatre))
        .collect();
    let map = match CampaignMap::load(&files, &loaded.info.map_key) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            error!("Campaign map {}: {e}", loaded.info.map_key);
            return;
        }
    };
    // The human faction: the one the front end chose, else the header's faction (the startpos
    // default, or the save's player; a save is already inside that faction's turn).
    let human = start.faction.clone().unwrap_or_else(|| loaded.info.header.faction_key.clone());
    if start.save.is_none() && !loaded.set_human(&human) {
        warn!("Campaign: no faction {human}; playing the header's faction");
        let h = loaded.info.header.faction_key.clone();
        loaded.set_human(&h);
    }
    let human = loaded.model.turn.humans.first().and_then(|f| loaded.model.world.factions.get(f)).map(|f| f.key.clone()).unwrap_or(human);
    let t = std::time::Instant::now();
    loaded.model.terrain = Some(Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map))));
    // The trade nodes' DB rows (their keys come from the map; CAMPAIGN_FIDELITY.md §Trade).
    ntw_campaign::trade::attach_map(&mut loaded.model, &map.regions);
    info!("Campaign: movement grid built in {:.0} ms", t.elapsed().as_secs_f32() * 1000.0);

    world.insert_resource(CampaignGround(map.clone()));
    // Faction colours from the `factions` table (primary colour).
    let colours: HashMap<FactionId, Color> = {
        let db = &world.resource::<GameData>().db;
        loaded
            .model
            .world
            .factions
            .values()
            .map(|f| {
                let c = db.faction(&f.key).map_or(Color::srgb(0.5, 0.5, 0.5), |r| {
                    let [r, g, b] = r.primary_colour();
                    Color::srgb_u8(r, g, b)
                });
                (f.id, c)
            })
            .collect()
    };
    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
        world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
            world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
                {
                    let mut commands = world.commands();
                    spawn_terrain(&mut commands, &files, &map, &mut meshes, &mut materials, &mut images);
                    spawn_lines(&mut commands, &map, &mut meshes, &mut materials);
                    spawn_rivers(&mut commands, &files, &map, &mut meshes, &mut materials, &mut images);
                    spawn_coast(&mut commands, &files, &map, &mut meshes, &mut materials, &mut images);
                    // The region names painted on the map (`theatres_and_region_keys` label
                    // positions + the loc `regions_onscreen_` names).
                    super::region_labels::spawn(&mut commands, &vfs, &map, &mut meshes, &mut materials, &mut images);
                    // The movement arrow's texture and chain pool (`display\arrows` + the
                    // CampaignPieces arrow texture).
                    super::arrows::load(&mut commands, &files, &map, &mut materials, &mut images);
                }
                let mut walls = SettlementWalls::from_db(&world.resource::<GameData>().db);
                let markers = spawn_static(world, &dir, &mut walls, &colours, &loaded.model, &map, &mut meshes, &mut materials, &mut images);
                world.insert_resource(markers);
                world.insert_resource(walls);
            });
        });
    });
    world.flush();

    // Camera: centred on the human player's faction (its first army, else its first settlement).
    let human_id = loaded.model.faction_by_key(&human).map(|f| f.id);
    let m = &loaded.model;
    let focus = human_id
        .and_then(|id| {
            m.world.forces.values().filter(|f| f.faction == id && !f.is_navy).find_map(|f| {
                let c = m.world.characters.get(&f.commander?)?;
                Some((c.position.0.to_f32(), c.position.1.to_f32()))
            })
        })
        .or_else(|| m.world.regions.values().find(|r| Some(r.owner) == human_id).map(|r| (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32())))
        .unwrap_or((0.0, 0.0));
    let (tmin, tmax) = map.regions.theatre;
    let mut cam = CampaignCamera {
        target: Vec2::new(focus.0, -focus.1),
        distance: 60.0,
        min: Vec2::new(tmin.0, -tmax.1),
        max: Vec2::new(tmax.0, -tmin.1),
        ground: map.height_at(focus.0, focus.1),
    };
    // Test harness: `NAPOLEON_CAMPAIGN_CAMERA=x,z,distance` (logic units) overrides the start view.
    if let Ok(s) = std::env::var("NAPOLEON_CAMPAIGN_CAMERA") {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, z, d] = v[..] {
            cam.target = Vec2::new(x, -z);
            cam.distance = d.clamp(camera::MIN_DISTANCE, camera::MAX_DISTANCE);
        }
    }
    world.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: camera::FOV_DEGREES.to_radians(), near: 0.1, far: 5000.0, ..default() }),
        cam.transform(),
        cam,
        DespawnOnExit(GameMode::Campaign),
    ));
    // PROVISIONAL light (the map's `environment\*.lighting` is not read yet).
    world.spawn((
        DirectionalLight { illuminance: 6_000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(-30.0, 80.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(GameMode::Campaign),
    ));
    world.insert_resource(GlobalAmbientLight { brightness: 900.0, ..default() });
    world.insert_resource(ClearColor(Color::srgb(0.08, 0.06, 0.04)));

    // The scripts: the original campaign Lua runs against the model from here on.
    // The scripts' own `save_value` slots of a save (EPISODIC_RESTRICTIONS/LUA[]).
    let saved_script_values = super::play::script_values_in(&loaded.script_values);
    let restricted_units = loaded.restricted_units;
    let model = loaded.model;
    let saved_restricted_levels = model.world.restricted_buildings.len();
    let source = match ScriptSource::from_install(&dir) {
        Ok(s) => s,
        Err(e) => {
            error!("Campaign scripts unavailable: {e}");
            return;
        }
    };
    let mut host = match ScriptHost::new(model, &human, source) {
        Ok(h) => h,
        Err(e) => {
            error!("Campaign script host failed: {e}");
            return;
        }
    };
    if let Err(e) = host.load_campaign(&start.campaign) {
        warn!("Campaign scripts of {}: {e}", start.campaign);
    }
    // A save's restricted units (EPISODIC_RESTRICTIONS): the scripts set them only on a new game
    // (InitialiseCampaign at UICreated); the save keeps them. The restricted building levels are
    // already in the loaded model. Logged for a save only, with the save's own counts.
    if start.save.is_some() {
        info!("Campaign: {saved_restricted_levels} restricted building levels and {} units from the save", restricted_units.len());
    }
    host.state_mut().restricted_units.extend(restricted_units);
    // The campaign AI plays the non-human factions in the turn loop (`--campaign-ai on|off`).
    crate::campaign_ai::attach(world, &mut host, &vfs, &bytes);
    // INFERRED engine order for a new campaign: NewSession, NewCampaignStarted, then turn 1.
    let first_events: &[&str] = if start.save.is_some() { &["NewSession"] } else { &["NewSession", "NewCampaignStarted"] };
    for &name in first_events {
        let r = host.fire(name, ScriptContext::for_faction(&human));
        for e in r.errors {
            warn!("Campaign script {name}: {e}");
        }
    }
    if start.save.is_some() {
        // `load_value` reads the save's slots in order (CAMPAIGN_DATA.md §4).
        let r = host.load_values(saved_script_values);
        info!("Campaign: save loaded as {human} (LoadingGame: {} handlers, {} errors)", r.handlers, r.errors.len());
    } else {
        let started = host.start_campaign();
        let errors: usize = started.iter().map(|(_, r)| r.errors.len()).sum();
        info!("Campaign: turn 1 started as {human} ({} script events, {errors} script errors)", started.len());
    }
    let demo = std::env::args().any(|a| a == "--campaign-demo");
    let mut sim = CampaignSim {
        host,
        campaign: start.campaign.clone(),
        human,
        source: Arc::new(bytes),
        generation: 0,
        selected: None,
        selected_region: None,
        selected_fort: None,
        moves: Vec::new(),
        last_message: String::new(),
        demo_target: None,
        names: ntw_campaign::names::NameData::load(&vfs, &world.resource::<GameData>().db).map(Arc::new),
        pictures: Arc::new(pictures),
    };
    if demo {
        super::demo(&mut sim);
    }
    world.insert_non_send(sim);
    super::hud::enter(world);
}

/// Settlements (with their owner flags) and the marker assets.
#[allow(clippy::too_many_arguments)]
fn spawn_static(
    world: &mut World,
    dir: &std::path::Path,
    walls: &mut SettlementWalls,
    colours: &HashMap<FactionId, Color>,
    model: &CampaignModel,
    map: &CampaignMap,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> MarkerAssets {
    let mut factions = HashMap::new();
    for (&id, &colour) in colours {
        factions.insert(id, (colour, materials.add(StandardMaterial { base_color: colour, ..default() })));
    }
    let colour_of = |f: FactionId| factions.get(&f).map_or(Color::srgb(0.5, 0.5, 0.5), |c| c.0);

    let models = ModelSource::open(dir).map_err(|e| warn!("Campaign models unavailable: {e}")).ok();
    let mut cache = ModelCache::default();
    let mut commands = world.commands();

    // Load every settlement's fortification meshes up front, for **both** walled levels, so
    // `sync_walls` can put walls on a settlement the player builds them on later in the session.
    let mut wall_models = 0;
    if let Some(src) = &models {
        let slots: std::collections::BTreeSet<u32> = map.regions.regions.iter().filter_map(|r| settlement_slot_number(map, &r.key)).collect();
        wall_models = warm_walls(walls, src, slots.into_iter(), &mut cache, meshes, materials, images);
    }

    // Settlements.
    let mut placed = 0;
    let mut with_walls = 0;
    for region in model.world.regions.values() {
        let (x, z) = (region.settlement.position.0.to_f32(), region.settlement.position.1.to_f32());
        let h = map.height_at(x, z);
        let slot_no = settlement_slot_number(map, &region.key);
        let template = settlement_template(map, &region.key);
        let parts = match (&models, &template) {
            (Some(src), Some(path)) => cache.get(src, path, meshes, materials, images),
            _ => Vec::new(),
        };
        let flag = materials.add(StandardMaterial { base_color: colour_of(region.owner), unlit: true, ..default() });
        let pole = materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.2, 0.15), ..default() });
        let (pole_mesh, flag_mesh) = (meshes.add(Cylinder::new(0.06, 2.4)), meshes.add(Cuboid::new(1.0, 0.6, 0.05)));
        let region_id = region.id;
        commands
            .spawn((
                Transform::from_translation(bevy_pos(x, h, z)),
                Visibility::default(),
                Name::new(format!("settlement {}", region.settlement.key)),
                DespawnOnExit(GameMode::Campaign),
            ))
            .with_children(|c| {
                let s = Vec3::splat(DISPLAY_TO_LOGIC);
                for (mesh, mat) in &parts {
                    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_scale(s)));
                }
                // PLACEHOLDER owner marker: a faction-coloured flag pole (the original shows a flag).
                c.spawn((Mesh3d(pole_mesh), MeshMaterial3d(pole), Transform::from_xyz(0.0, 1.2, 0.0)));
                c.spawn((Mesh3d(flag_mesh), MeshMaterial3d(flag), Transform::from_xyz(0.5, 2.1, 0.0), SettlementFlag { region: region_id }));
            });
        if !parts.is_empty() {
            placed += 1;
        }
        // The settlement's own walls, at the file level its fortification building calls for.
        let drawn = walls.level_of(region.fortification.as_ref().map(|b| b.level_key.as_str()));
        let wall_parts = match (&models, slot_no, drawn) {
            (Some(src), Some(n), Some(level)) => walls.get(src, n, level, |p| cache.get(src, p, meshes, materials, images)),
            _ => None,
        };
        if wall_parts.is_some() {
            with_walls += 1;
        }
        let mut children = Vec::new();
        let parent = commands
            .spawn((
                Transform::from_translation(bevy_pos(x, h, z)).with_scale(Vec3::splat(DISPLAY_TO_LOGIC)),
                if wall_parts.is_some() { Visibility::Visible } else { Visibility::Hidden },
                Name::new(format!("settlement walls {}", region.settlement.key)),
                DespawnOnExit(GameMode::Campaign),
            ))
            .with_children(|c| {
                for (mesh, mat) in wall_parts.iter().flatten() {
                    children.push(c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()))).id());
                }
            })
            .id();
        // The ids come back from `with_children`, so the view (which needs them, to replace exactly
        // these parts when the level changes) is inserted afterwards.
        commands.entity(parent).insert(SettlementWallsView { region: region_id, slot: slot_no.unwrap_or(0), level: drawn, parts: children });
    }
    info!("Campaign: {} settlements ({placed} with an original model)", model.world.regions.len());
    info!("Campaign: settlement fortification art for {with_walls} of them ({wall_models} walled meshes loaded, {} cached)", walls.parts.len());

    // Town, port and other slot templates: each regions.esf slot names its template model
    // (`MapSlot::model`, e.g. nap_eur_port, CONFIRMED key), placed at the slot's position turned by
    // its angle. INFERRED: the file is `templates\<culture>\<key>.rigid_model` (the eu folder is
    // tried first), the angle turns the model about the vertical with 65536 = one turn.
    // Direction (CONFIRMED by the data): every port slot's `dock` point lies at
    // (−sin θ, −cos θ) × 1.3 from its position in logic (x, z) (all 63 ports on the four maps within
    // 6°), so the model's dock side, south at θ = 0, turns clockwise seen from above: a Bevy
    // rotation of −θ about +Y. Towns use the same slot angle (no dock to check).
    let mut slot_models = 0;
    if let Some(src) = &models {
        for region in map.regions.regions.iter() {
            let Some(settlement) = &region.settlement else { continue };
            for slot in settlement.slots.iter().filter(|s| !s.model.is_empty() && !s.slot_type.starts_with("settlement")) {
                let key = slot.model.to_ascii_lowercase();
                let path = ["eu", "ott", "ind", "na"]
                    .iter()
                    .map(|c| format!("rigidmodels\\campaignbuildings\\templates\\{c}\\{key}.rigid_model"))
                    .find(|p| src.vfs.contains(p));
                let Some(path) = path else { continue };
                let parts = cache.get(src, &path, meshes, materials, images);
                if parts.is_empty() {
                    continue;
                }
                let (x, z) = (slot.position.0, slot.position.2);
                let turn = -(slot.angle as f32 / 65536.0 * std::f32::consts::TAU);
                commands
                    .spawn((
                        Transform::from_translation(bevy_pos(x, map.height_at(x, z), z)).with_rotation(Quat::from_rotation_y(turn)),
                        Visibility::default(),
                        Name::new(format!("slot {}", slot.key)),
                        DespawnOnExit(GameMode::Campaign),
                    ))
                    .with_children(|c| {
                        for (mesh, mat) in &parts {
                            c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_scale(Vec3::splat(DISPLAY_TO_LOGIC))));
                        }
                    });
                slot_models += 1;
            }
        }
    }
    info!("Campaign: {slot_models} town / port slot models placed");

    // Trees (`display\trees\campaign.rigid_trees`, CONFIRMED reader `0x00966500`): every instance
    // stands at its stored logic position with its stored uniform scale (which also turns the
    // model's display units into logic units). The exe turns each one by a CRT `rand()` value;
    // PROVISIONAL: a fixed pseudo-random angle from the instance's position. The exe also loads a
    // `_low.rigid_model` variant beside each model for distance (the switch rule is UNKNOWN; the
    // full model is drawn).
    let mut trees = 0;
    // `NAPOLEON_CAMPAIGN_TREES=0` leaves them out (for FPS comparisons).
    let trees_on = std::env::var("NAPOLEON_CAMPAIGN_TREES").map_or(true, |v| v != "0");
    if let (Some(src), Some(list), true) = (&models, &map.trees, trees_on) {
        for model in &list.models {
            let path = model.path.replace('/', "\\").to_ascii_lowercase();
            let instances: Vec<_> = model.groups.iter().flat_map(|g| &g.instances).collect();
            if instances.is_empty() || !src.vfs.contains(&path) {
                continue;
            }
            let parts = cache.get(src, &path, meshes, materials, images);
            if parts.is_empty() {
                continue;
            }
            for i in instances {
                let [x, y, z] = i.position;
                let hash = (x.to_bits() ^ z.to_bits().rotate_left(13)).wrapping_mul(0x9E37_79B9);
                let turn = (hash >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU;
                commands
                    .spawn((
                        Transform::from_translation(bevy_pos(x, y, z))
                            .with_rotation(Quat::from_rotation_y(turn))
                            .with_scale(Vec3::splat(i.scale)),
                        Visibility::default(),
                        DespawnOnExit(GameMode::Campaign),
                    ))
                    .with_children(|c| {
                        for (mesh, mat) in &parts {
                            c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
                        }
                    });
                trees += 1;
            }
        }
    }
    info!("Campaign: {trees} trees placed");

    let mut badges = HashMap::new();
    if let Some(src) = &models {
        for f in model.world.factions.values() {
            let path = format!("rigidmodels\\campaignbuildings\\campaignagenticons\\agent_id_{}.rigid_model", f.key.to_ascii_lowercase());
            if src.vfs.contains(&path) {
                badges.insert(f.id, cache.get(src, &path, meshes, materials, images));
            }
        }
    }
    MarkerAssets {
        army: meshes.add(Cone { radius: 0.55, height: 1.6 }),
        navy: meshes.add(Cuboid::new(1.6, 0.5, 0.7)),
        agent: meshes.add(Sphere::new(0.35)),
        factions,
        badges,
        grey: materials.add(StandardMaterial { base_color: Color::srgb(0.5, 0.5, 0.5), ..default() }),
    }
}

/// Swaps a settlement's fortification mesh when the model builds, improves or demolishes its
/// walls. Gated on the model's generation like `play::sync_markers`, so it only looks at the
/// model after a command has been applied.
pub fn sync_walls(
    mut commands: Commands,
    sim: Option<NonSend<CampaignSim>>,
    walls: Option<Res<SettlementWalls>>,
    mut synced: Local<u64>,
    mut views: Query<(Entity, &mut SettlementWallsView, &mut Visibility)>,
) {
    let (Some(sim), Some(walls)) = (sim, walls) else { return };
    if *synced == sim.generation + 1 {
        return;
    }
    *synced = sim.generation + 1;
    let m = sim.model();
    let mut changed = 0;
    for (entity, mut view, mut vis) in &mut views {
        let want = m
            .world
            .regions
            .get(&view.region)
            .and_then(|r| walls.level_of(r.fortification.as_ref().map(|b| b.level_key.as_str())));
        if want == view.level {
            continue;
        }
        let parts = want.and_then(|level| walls.parts_of(view.slot, level).cloned());
        for old in view.parts.drain(..) {
            commands.entity(old).despawn();
        }
        match parts {
            Some(p) => {
                commands.entity(entity).with_children(|c| {
                    for (mesh, mat) in &p {
                        view.parts.push(
                            c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default())).id(),
                        );
                    }
                });
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
        view.level = want;
        changed += 1;
    }
    if changed > 0 {
        info!("Campaign: {changed} settlement fortification meshes updated");
    }
}

/// The settlement template model for a region: `rigidmodels\campaignbuildings\templates\eu\
/// eu_city_<n>_slot.rigid_model`, where `<n>` comes from the region's `settlement_<n>_slot`
/// slot type in `regions.esf`. INFERRED from the names; the culture prefix `eu` is
/// PROVISIONAL (the `ott`, `ind`, `na` sets exist too).
fn settlement_template(map: &CampaignMap, region_key: &str) -> Option<String> {
    let n = settlement_slot_number(map, region_key)?;
    Some(format!("rigidmodels\\campaignbuildings\\templates\\eu\\{SETTLEMENT_CITY_STEM}_{n}_slot.rigid_model"))
}

/// The template stem the plain settlement mesh is drawn from (`eu_city`, PROVISIONAL: see
/// [`settlement_template`]). The walls prefer the same stem so the two always match.
const SETTLEMENT_CITY_STEM: &str = "eu_city";

/// The `n` of a region's `settlement_<n>_slot` slot type, which numbers every mesh of that
/// settlement (`eu_city_<n>_slot`, `eu_city_<n>_slot_fortifications_lvl<level>`).
fn settlement_slot_number(map: &CampaignMap, region_key: &str) -> Option<u32> {
    let region = map.regions.regions.iter().find(|r| r.key == region_key)?;
    region.settlement.as_ref()?.slots.iter().find_map(|s| {
        s.slot_type.strip_prefix("settlement_")?.strip_suffix("_slot")?.parse::<u32>().ok()
    })
}

type Parts = Vec<(Handle<Mesh>, Handle<StandardMaterial>)>;

#[derive(Default)]
struct ModelCache {
    models: HashMap<String, Parts>,
    textures: HashMap<String, Handle<Image>>,
}

impl ModelCache {
    fn get(
        &mut self,
        src: &ModelSource,
        path: &str,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Parts {
        if let Some(p) = self.models.get(path) {
            return p.clone();
        }
        let mut parts = Vec::new();
        match src.vfs.read(path).map_err(|e| e.to_string()).and_then(|b| RigidModel::read(&b).map_err(|e| e.to_string())) {
            Ok(model) => {
                let flags = NameFlags::from_path(path);
                for rm in &model.meshes {
                    let arrays = convert_mesh(rm);
                    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
                    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, arrays.positions);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, arrays.normals);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, arrays.uvs);
                    mesh.insert_indices(Indices::U32(arrays.indices));
                    let texture = rm.material.diffuse_name().and_then(|n| src.find_texture(path, &n)).and_then(|hit| {
                        let p = hit.path().to_owned();
                        if let Some(h) = self.textures.get(&p) {
                            return Some(h.clone());
                        }
                        let h = images.add(load_dds(src, &p).map_err(|e| warn!("{p}: {e}")).ok()?);
                        self.textures.insert(p, h.clone());
                        Some(h)
                    });
                    let material = StandardMaterial {
                        base_color: if texture.is_some() { Color::WHITE } else { Color::srgb(0.6, 0.6, 0.6) },
                        base_color_texture: texture,
                        perceptual_roughness: 0.9,
                        reflectance: 0.1,
                        double_sided: flags.two_sided,
                        cull_mode: if flags.two_sided { None } else { Some(Face::Back) },
                        alpha_mode: if flags.alpha_blend {
                            AlphaMode::Blend
                        } else if flags.alpha_test {
                            AlphaMode::Mask(0.5)
                        } else {
                            AlphaMode::Opaque
                        },
                        ..default()
                    };
                    parts.push((meshes.add(mesh), materials.add(material)));
                }
            }
            Err(e) => warn!("{path}: {e}"),
        }
        self.models.insert(path.to_owned(), parts.clone());
        parts
    }
}

/// The terrain: one grid mesh over the whole map bounds with the supertexture as its colour.
fn spawn_terrain(
    commands: &mut Commands,
    files: &GameFiles<'_>,
    map: &CampaignMap,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let hm = &map.heightmap;
    let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
    let (cols, rows) = (hm.width / TERRAIN_STRIDE + 1, hm.height / TERRAIN_STRIDE + 1);
    let (sx, sz) = ((mx.0 - mn.0) / (cols - 1) as f32, (mx.1 - mn.1) / (rows - 1) as f32);
    let mut positions = Vec::with_capacity((cols * rows) as usize);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    let height = |c: i64, r: i64| {
        let (c, r) = (c.clamp(0, cols as i64 - 1), r.clamp(0, rows as i64 - 1));
        hm.sample(c as f32 / (cols - 1) as f32 * hm.width as f32, r as f32 / (rows - 1) as f32 * hm.height as f32) * HEIGHT_SCALE
    };
    for r in 0..rows as i64 {
        for c in 0..cols as i64 {
            let (u, v) = (c as f32 / (cols - 1) as f32, r as f32 / (rows - 1) as f32);
            let x = mn.0 + u * (mx.0 - mn.0);
            let z = mx.1 - v * (mx.1 - mn.1);
            positions.push(bevy_pos(x, height(c, r), z).to_array());
            // Bevy +X = east (columns), +Z = south (rows).
            let gx = (height(c + 1, r) - height(c - 1, r)) / (2.0 * sx);
            let gz = (height(c, r + 1) - height(c, r - 1)) / (2.0 * sz);
            normals.push(Vec3::new(-gx, 1.0, -gz).normalize().to_array());
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::with_capacity(((cols - 1) * (rows - 1) * 6) as usize);
    for r in 0..rows - 1 {
        for c in 0..cols - 1 {
            let (a, b, d, e) = (r * cols + c, r * cols + c + 1, (r + 1) * cols + c, (r + 1) * cols + c + 1);
            indices.extend_from_slice(&[a, d, b, b, d, e]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    let texture = map.supertexture.as_ref().and_then(|st| {
        let level = st.levels.iter().position(|l| l.tiles_x * st.tile_size <= MAX_TEXTURE_WIDTH)?;
        match map.supertexture_rgba(files, level) {
            Ok((w, h, mut rgba)) => {
                // The alpha channel is a mask of UNKNOWN use; the colour is drawn opaque.
                rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
                info!("Campaign map: supertexture level {level} ({w}x{h})");
                Some(images.add(image_with_mips(w, h, rgba)))
            }
            Err(e) => {
                warn!("Campaign map supertexture: {e}");
                None
            }
        }
    });
    let material = StandardMaterial {
        base_color: if texture.is_some() { Color::WHITE } else { Color::srgb(0.35, 0.45, 0.3) },
        base_color_texture: texture,
        perceptual_roughness: 1.0,
        reflectance: 0.05,
        ..default()
    };
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        Transform::default(),
        Name::new("campaign terrain"),
        DespawnOnExit(GameMode::Campaign),
    ));
}

/// Samples per cubic Bezier segment when a spline is drawn.
const BEZIER_STEPS: usize = 8;
/// River ribbon width in logic units: the river builder `0x0111A080` passes 1.5 to `0x0116E410`,
/// which builds the ribbon from −0.5 to +0.5 times it across the curve (CONFIRMED).
const RIVER_WIDTH: f32 = 1.5;

/// A campaign spline's points in logic (x, z), evaluated as the cubic Bezier curves they are: every
/// `.rigid_spline` on the five maps has 3k+1 points, and the river builder reads them as (k)
/// segments of four control points (`0x0111A080` → `0x005AFED0`, CONFIRMED), scaling display units
/// by 39.37008 (the exe's own constant, = `DISPLAY_TO_LOGIC`).
fn bezier_points(points: &[[f32; 3]]) -> Vec<(f32, f32)> {
    let p: Vec<(f32, f32)> = points.iter().map(|q| (q[0] * DISPLAY_TO_LOGIC, q[2] * DISPLAY_TO_LOGIC)).collect();
    if p.len() < 4 || !(p.len() - 1).is_multiple_of(3) {
        return p;
    }
    let mut out = vec![p[0]];
    for seg in p.windows(4).step_by(3) {
        for i in 1..=BEZIER_STEPS {
            let t = i as f32 / BEZIER_STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            out.push((
                a * seg[0].0 + b * seg[1].0 + c * seg[2].0 + d * seg[3].0,
                a * seg[0].1 + b * seg[1].1 + c * seg[2].1 + d * seg[3].1,
            ));
        }
    }
    out
}

/// A DDS file's image (all mips, sRGB, repeating) from its bytes.
fn dds_image(bytes: &[u8]) -> Result<Image, String> {
    let dds = ntw_formats::dds::Dds::parse(bytes).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    for level in 0..dds.mip_count {
        data.extend(dds.decode_rgba8(level));
    }
    let size = Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, dds.decode_rgba8(0), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = dds.mip_count;
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

/// The surf along the coasts: the map's `coastline_group<n>.rigid_mesh` strips (1 unit wide) drawn
/// with `fx\campaignterrain.fx` technique `sm3_campaign_coast` (CONFIRMED source): colour 0.65 grey,
/// unlit; height 0.1; the texture `Campaign\Coastline\waves` sampled at `(0.5 · tex2.u, 1.7 · (tex.v −
/// 0.5) + 0.5)` (wrap along, clamp across); alpha = `saturate(a^1.25 · 1.5)` of two time-shifted
/// wave samples, each a blend of the texture's red, green and blue frames, times the end fade
/// `min(saturate(t / 0.15), saturate((1 − t) / 0.15))` with `t = tex2.u / tex2.v`; blended over the
/// sea without depth writes. Here one still frame (PROVISIONAL): the waves' alpha is baked into the
/// texture from `0.25 r + 0.25 g + 0.5 b` (the frame blend at its mid point, both waves at half
/// strength), the end fade goes in the vertex colours, and nothing moves.
fn spawn_coast(
    commands: &mut Commands,
    files: &GameFiles<'_>,
    map: &CampaignMap,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    // `NAPOLEON_CAMPAIGN_COAST=0` leaves the surf out (for comparisons).
    if map.coast.is_empty() || std::env::var("NAPOLEON_CAMPAIGN_COAST").is_ok_and(|v| v == "0") {
        return;
    }
    let texture = files.read("campaign/coastline/waves.dds").and_then(|b| {
        let dds = ntw_formats::dds::Dds::parse(&b).map_err(|e| e.to_string())?;
        let mut rgba = dds.decode_rgba8(0);
        for p in rgba.as_chunks_mut::<4>().0 {
            let w = (0.25 * p[0] as f32 + 0.25 * p[1] as f32 + 0.5 * p[2] as f32) / 255.0;
            let a = (w.powf(1.25) * 1.5).clamp(0.0, 1.0);
            p.copy_from_slice(&[166, 166, 166, (a * 255.0) as u8]);
        }
        let mut image = image_with_mips(dds.width, dds.height, rgba);
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
        Ok(image)
    });
    let texture = texture.map_err(|e| warn!("Campaign coast texture: {e}")).ok().map(|i| images.add(i));
    for c in &map.coast {
        let n = c.vertices.len();
        let pos: Vec<[f32; 3]> = c.vertices.iter().map(|v| bevy_pos(v.position[0], 0.1, v.position[2]).to_array()).collect();
        let uv: Vec<[f32; 2]> = c.vertices.iter().map(|v| [v.tex2[0] * 0.5, (v.tex[1] - 0.5) * 1.7 + 0.5]).collect();
        let col: Vec<[f32; 4]> = c
            .vertices
            .iter()
            .map(|v| {
                let t = if v.tex2[1] > 0.0 { v.tex2[0] / v.tex2[1] } else { 0.5 };
                [1.0, 1.0, 1.0, (t / 0.15).clamp(0.0, 1.0).min(((1.0 - t) / 0.15).clamp(0.0, 1.0))]
            })
            .collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        mesh.insert_indices(Indices::U32(c.indices.clone()));
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: texture.clone(),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                depth_bias: 1.0,
                ..default()
            })),
            Name::new("campaign coastline"),
            DespawnOnExit(GameMode::Campaign),
        ));
    }
}

/// River ribbons (`fx\campaignriver.fx`, the river builder `0x0111A080`): each river spline's Bezier
/// curve as a 1.5-unit ribbon textured with `display\rivers\textures\river_diffuse.dds`. The shader
/// (CONFIRMED source) samples the texture at `(0.875 v, 0.1 s)` and `(0.7 v, 0.15 s)`, `s` = distance
/// along the river and `v` = 0..1 across, scrolls them with time and averages them; its alpha is
/// `saturate(s) · saturate(len − s) · (1 − |2v − 1|)^0.2` (ends fade over one unit, edges soft); it
/// lights with an up normal and draws without depth test at height 0. Here: the first sample only (no
/// scrolling, PROVISIONAL), the alpha in the vertex colours over five vertices across, and the ribbon
/// laid on the terrain (depth-tested, lifted a little) instead of drawn over it at height 0.
fn spawn_rivers(
    commands: &mut Commands,
    files: &GameFiles<'_>,
    map: &CampaignMap,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let texture = files
        .read(&format!("campaign_maps/{}/display/rivers/textures/river_diffuse.dds", map.name))
        .and_then(|b| dds_image(&b))
        .map_err(|e| warn!("Campaign river texture: {e}"))
        .ok()
        .map(|i| images.add(i));
    let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::<u32>::new());
    let across = [0.0f32, 0.1, 0.5, 0.9, 1.0];
    for (_, s) in map.splines.iter().filter(|(f, _)| f == "rivers") {
        let pts = bezier_points(&s.points);
        if pts.len() < 2 {
            continue;
        }
        let mut dist = vec![0.0f32];
        for w in pts.windows(2) {
            let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            dist.push(dist.last().copied().unwrap_or(0.0) + d);
        }
        let len = *dist.last().unwrap_or(&0.0);
        let base = pos.len() as u32;
        for (i, &(x, z)) in pts.iter().enumerate() {
            let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(pts.len() - 1)]);
            let (tx, tz) = (b.0 - a.0, b.1 - a.1);
            let tl = (tx * tx + tz * tz).sqrt().max(1e-6);
            let (nx, nz) = (-tz / tl, tx / tl);
            let ends = dist[i].clamp(0.0, 1.0) * (len - dist[i]).clamp(0.0, 1.0);
            for &v in &across {
                let off = (v - 0.5) * RIVER_WIDTH;
                let (px, pz) = (x + nx * off, z + nz * off);
                pos.push(bevy_pos(px, map.height_at(px, pz).max(0.0) + LINE_LIFT, pz).to_array());
                uv.push([v * 0.875, dist[i] * 0.1]);
                let ay = (1.0 - (2.0 * v - 1.0).abs()).max(0.0).powf(0.2);
                col.push([1.0, 1.0, 1.0, ends * ay]);
            }
        }
        let k = across.len() as u32;
        for i in 0..pts.len() as u32 - 1 {
            for j in 0..k - 1 {
                let (a, b) = (base + i * k + j, base + (i + 1) * k + j);
                idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
    if idx.is_empty() {
        return;
    }
    let n = pos.len();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: texture,
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            reflectance: 0.05,
            cull_mode: None,
            ..default()
        })),
        Name::new("campaign rivers"),
        DespawnOnExit(GameMode::Campaign),
    ));
}

/// Borders and roads as plain lines along their Bezier curves (PLACEHOLDER styling: the original
/// draws textured ribbons).
fn spawn_lines(commands: &mut Commands, map: &CampaignMap, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    for (folder, colour) in [("borders", Color::srgb(0.95, 0.85, 0.55)), ("roads", Color::srgb(0.45, 0.32, 0.18))] {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        for (_, s) in map.splines.iter().filter(|(f, _)| f == folder) {
            let pts: Vec<Vec3> = bezier_points(&s.points)
                .into_iter()
                .map(|(x, z)| bevy_pos(x, map.height_at(x, z).max(0.0) + LINE_LIFT, z))
                .collect();
            for w in pts.windows(2) {
                positions.push(w[0].to_array());
                positions.push(w[1].to_array());
            }
        }
        if positions.is_empty() {
            continue;
        }
        let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: colour, unlit: true, ..default() })),
            Name::new(format!("campaign {folder}")),
            DespawnOnExit(GameMode::Campaign),
        ));
    }
}

/// RGBA8 image with a box-filtered mip chain, clamped, trilinear.
pub(super) fn image_with_mips(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    let mut data = rgba.clone();
    let (mut w, mut h, mut level) = (width as usize, height as usize, rgba);
    let mut mips = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for ch in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                        s += u32::from(level[(sy * w + sx) * 4 + ch]);
                    }
                    next[(y * nw + x) * 4 + ch] = (s / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        (w, h, level) = (nw, nh, next);
        mips += 1;
    }
    let size = Extent3d { width, height, depth_or_array_layers: 1 };
    let first = data[..(width * height * 4) as usize].to_vec();
    let mut image = Image::new(size, TextureDimension::D2, first, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mips;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Test harness: with `NAPOLEON_FPS_LOG=<seconds>`, logs the campaign map's frame rate every 2 s
/// (mean FPS, worst frame; vsync off) and quits after that many seconds of real time.
pub fn fps_log(
    time: Res<Time<Real>>,
    mut acc: Local<(f32, u32, f32, f32)>,
    mut exit: MessageWriter<AppExit>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(limit) = std::env::var("NAPOLEON_FPS_LOG").ok().and_then(|s| s.parse::<f32>().ok()) else { return };
    for mut w in &mut windows {
        if w.present_mode != bevy::window::PresentMode::AutoNoVsync {
            w.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
    }
    let dt = time.delta_secs();
    let (window, frames, worst, total) = &mut *acc;
    *window += dt;
    *frames += 1;
    *worst = worst.max(dt);
    *total += dt;
    if *window >= 2.0 {
        info!("Campaign FPS {:.1} (worst frame {:.1} ms), t = {:.0} s", *frames as f32 / *window, *worst * 1000.0, *total);
        (*window, *frames, *worst) = (0.0, 0, 0.0);
    }
    if *total >= limit {
        exit.write(AppExit::Success);
    }
}
