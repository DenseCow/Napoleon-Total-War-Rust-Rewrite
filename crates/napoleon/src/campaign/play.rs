//! Playing the campaign: the simulation resource, army markers that follow the model, selection,
//! right-click orders with a path preview, move animation, end turn, autoresolve and quick save.
//!
//! The model (`ntw_sim::campaign::CampaignModel`) lives inside the Lua [`ScriptHost`] so the
//! original campaign scripts see every event; every change goes through
//! [`ScriptHost::apply`] (a `CampaignCommand`). Lua is single-threaded, so the host is a non-`Send`
//! resource.
//!
//! Controls (PROVISIONAL bindings; the original's are in `text/default_keys.xml`, not read yet):
//! left click selects an army, navy or agent; right click (without dragging) orders: move to the
//! ground, attack an enemy army, merge with an own army, board an own fleet (an embarked army
//! lands where it is ordered), or enter a settlement. `Return` ends the
//! turn (the HUD button does too); `F5` writes a save to NapoleonRust's own folder.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_script::ScriptHost;
use ntw_sim::campaign::{CampaignCommand, CampaignEvent, CampaignModel, CharacterId, CharacterKind, FactionId, ForceId, RegionId};
use ntw_sim::fixed::Fixed20;

use super::camera::CampaignCamera;
use super::scene::{CampaignGround, MarkerAssets};

/// The running campaign. Non-`Send` (it owns the Lua state).
pub struct CampaignSim {
    /// The script host; it owns the model.
    pub host: ScriptHost,
    /// Campaign key, e.g. `eur_napoleon`.
    pub campaign: String,
    /// The human player's faction key.
    pub human: String,
    /// The file the campaign was loaded from (startpos or save), kept to write saves from.
    pub source: Arc<Vec<u8>>,
    /// Bumped after every change, so displays know when to refresh.
    pub generation: u64,
    /// The selected character.
    pub selected: Option<CharacterId>,
    /// The selected settlement (when no character is selected).
    pub selected_region: Option<RegionId>,
    /// The selected fort, by the region that holds it (when neither a character nor a settlement is
    /// selected; see `ntw_script::ui::CampaignSelection::Fort`). A map click picks a fort too
    /// (`map_pick`, [`fort_near`]), and the HUD and the harness `selectfort:<region key>` step can
    /// select one directly. **PROVISIONAL:** the region is the identity here, so two forts in one
    /// region cannot be told apart, and a fort whose record gave no position cannot be clicked.
    ///
    /// No shipped file has a fort -- `World::forts` (the saves' `REGION/FORT_ARRAY`) is empty in all
    /// eight start positions and all ten vanilla saves -- so in practice a fort selection still comes
    /// only from the HUD.
    pub selected_fort: Option<RegionId>,
    /// Paths to animate, by character (logic x, z), from the last command.
    pub moves: Vec<(CharacterId, Vec<(f32, f32)>)>,
    /// A short status line for the log and the harness.
    pub last_message: String,
    /// Harness (`--campaign-demo`): a fixed logic (x, z) for the path preview instead of the cursor.
    pub demo_target: Option<(f32, f32)>,
    /// Names data for naming new characters and unit officers in saves (`None`: template names).
    pub names: Option<Arc<ntw_campaign::names::NameData>>,
    /// The theatres' base and lookup pictures for the save header's territory maps
    /// (`ntw_campaign::header_map`).
    pub pictures: Arc<Vec<ntw_campaign::header_map::TheatrePictures>>,
}

impl CampaignSim {
    /// The model (borrowed from the script host; do not hold across Lua calls).
    pub fn model(&self) -> std::cell::Ref<'_, CampaignModel> {
        self.host.model()
    }

    /// The human faction's id.
    pub fn human_id(&self) -> Option<FactionId> {
        self.model().faction_by_key(&self.human).map(|f| f.id)
    }

    /// Applies a command through the script host, then autoresolves any battle it started
    /// (PROVISIONAL: the real-time battle is not launched yet — see `ntw_sim::campaign::battles`
    /// for the hook) and records moves to animate.
    pub fn command(&mut self, cmd: CampaignCommand) {
        let label = format!("{cmd:?}");
        match self.host.apply(cmd) {
            Ok(fired) => {
                let mut events: Vec<CampaignEvent> = fired.into_iter().map(|(e, _)| e).collect();
                if self.model().pending_battle.is_some() {
                    // HOOK: launch GameMode::Battle with the pending battle's armies here and
                    // apply its outcome with `CampaignModel::apply_battle_result`.
                    match self.host.apply(CampaignCommand::Autoresolve) {
                        Ok(f) => events.extend(f.into_iter().map(|(e, _)| e)),
                        Err(e) => warn!("Autoresolve: {e}"),
                    }
                }
                for e in &events {
                    if let CampaignEvent::CharacterMoved { character, path } = e {
                        self.moves.push((*character, path.clone()));
                    }
                    if let CampaignEvent::ConstructionItemDropped { region, slot, level_key } = e {
                        warn!("Campaign: dropped construction item {level_key} in region {region:?}: its slot {slot:?} does not exist");
                    }
                    if let CampaignEvent::BattleCompleted { attacker_won, .. } = e {
                        self.last_message = format!("Battle: the attacker {}", if *attacker_won { "won" } else { "lost" });
                        info!("{}", self.last_message);
                    }
                }
                if self.last_message.is_empty() || !label.starts_with("Attack") {
                    self.last_message = format!("{label}: ok");
                }
            }
            Err(e) => {
                self.last_message = format!("{label}: {e}");
                info!("Campaign: {}", self.last_message);
            }
        }
        self.generation += 1;
    }

    /// Ends the human's turn (the AI factions play, then the human's next turn starts).
    pub fn end_turn(&mut self) {
        self.command(CampaignCommand::EndTurn);
        let m = self.model();
        let msg = format!(
            "Turn {} ({} {}), treasury {}",
            m.calendar.turn_number(),
            month_name(m.calendar.date.month),
            m.calendar.date.year,
            m.faction_by_key(&self.human).map_or(0, |f| f.treasury)
        );
        drop(m);
        info!("Campaign: end turn -> {msg}");
        self.last_message = msg;
    }
}

/// English month name for logs.
pub fn month_name(m: u32) -> &'static str {
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
        .get(m as usize)
        .copied()
        .unwrap_or("?")
}

/// A character's marker on the map.
#[derive(Component)]
pub struct CharacterMarker {
    /// The character.
    pub id: CharacterId,
}

/// A settlement's owner flag (recoloured when the owner changes).
#[derive(Component)]
pub struct SettlementFlag {
    /// The region.
    pub region: RegionId,
}

/// A move being animated: the marker walks along `path` (Bevy positions).
#[derive(Component)]
pub struct MoveAnimation {
    path: Vec<Vec3>,
    /// Distance walked so far.
    done: f32,
}

/// Animation speed in map units per second. PLACEHOLDER (presentation only).
const WALK_SPEED: f32 = 25.0;

/// Logic (x, z) → Bevy position on the ground, lifted by `lift`.
fn ground_pos(ground: &CampaignGround, x: f32, z: f32, lift: f32) -> Vec3 {
    Vec3::new(x, ground.0.height_at(x, z).max(0.0) + lift, -z)
}

/// What a character's marker looks like: (lift above ground).
fn marker_lift(kind: CharacterKind, navy: bool, army: bool) -> f32 {
    if navy {
        0.25
    } else if army || kind == CharacterKind::General {
        0.8
    } else {
        0.35
    }
}

/// Keeps one marker per character on the map, at the model's position (or animating there),
/// and recolours settlement flags.
#[allow(clippy::too_many_arguments)]
pub fn sync_markers(
    mut commands: Commands,
    sim: Option<NonSendMut<CampaignSim>>,
    ground: Option<Res<CampaignGround>>,
    assets: Option<Res<MarkerAssets>>,
    mut synced: Local<u64>,
    mut markers: Query<(Entity, &CharacterMarker, &mut Transform, Option<&mut MoveAnimation>)>,
    flags: Query<(&SettlementFlag, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(mut sim), Some(ground), Some(assets)) = (sim, ground, assets) else { return };
    if *synced == sim.generation + 1 {
        return;
    }
    *synced = sim.generation + 1;
    let moves = std::mem::take(&mut sim.moves);
    let m = sim.model();
    let navies: HashMap<CharacterId, bool> =
        m.world.forces.values().filter_map(|f| f.commander.map(|c| (c, f.is_navy))).collect();
    let mut seen = std::collections::HashSet::new();
    for (e, marker, mut tf, anim) in &mut markers {
        let Some(ch) = m.world.characters.get(&marker.id) else {
            commands.entity(e).despawn();
            continue;
        };
        seen.insert(marker.id);
        let navy = navies.get(&ch.id).copied();
        let lift = marker_lift(ch.kind, navy == Some(true), navy == Some(false));
        let (x, z) = (ch.position.0.to_f32(), ch.position.1.to_f32());
        let target = ground_pos(&ground, x, z, lift);
        if let Some((_, path)) = moves.iter().find(|(c, _)| *c == ch.id) {
            let pts: Vec<Vec3> = path.iter().map(|&(px, pz)| ground_pos(&ground, px, pz, lift)).collect();
            commands.entity(e).insert(MoveAnimation { path: pts, done: 0.0 });
        } else if anim.is_none() {
            tf.translation = target;
        }
    }
    for ch in m.world.characters.values() {
        if seen.contains(&ch.id) || (ch.position.0.raw() == 0 && ch.position.1.raw() == 0) {
            continue;
        }
        let navy = navies.get(&ch.id).copied();
        let lift = marker_lift(ch.kind, navy == Some(true), navy == Some(false));
        let pos = ground_pos(&ground, ch.position.0.to_f32(), ch.position.1.to_f32(), lift);
        assets.spawn_marker(&mut commands, ch.id, ch.faction, navy, ch.kind, pos);
    }
    for (flag, mat) in &flags {
        let Some(r) = m.world.regions.get(&flag.region) else { continue };
        let want = assets.colour(r.owner);
        if materials.get(&mat.0).is_some_and(|x| x.base_color != want)
            && let Some(mut x) = materials.get_mut(&mat.0)
        {
            x.base_color = want;
        }
    }
}

/// Walks animated markers along their paths.
pub fn animate(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Transform, &mut MoveAnimation)>) {
    for (e, mut tf, mut anim) in &mut q {
        anim.done += WALK_SPEED * time.delta_secs();
        let mut left = anim.done;
        let mut pos = *anim.path.last().unwrap_or(&tf.translation);
        let mut finished = true;
        for w in anim.path.windows(2) {
            let len = w[0].distance(w[1]);
            if left <= len {
                pos = w[0].lerp(w[1], if len > 0.0 { left / len } else { 1.0 });
                finished = false;
                break;
            }
            left -= len;
        }
        tf.translation = pos;
        if finished {
            commands.entity(e).remove::<MoveAnimation>();
        }
    }
}

/// The ground point under the cursor, in logic (x, z).
fn cursor_ground(
    window: &Window,
    camera: &Camera,
    cam_tf: &GlobalTransform,
    ground: &CampaignGround,
) -> Option<(f32, f32)> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
    // Intersect with the terrain: start at y = 0, refine with the height there (three passes are
    // plenty for the gentle campaign heightmap).
    let mut h = 0.0;
    let mut p = Vec3::ZERO;
    for _ in 0..3 {
        if ray.direction.y.abs() < 1e-5 {
            return None;
        }
        let t = (h - ray.origin.y) / ray.direction.y;
        if t < 0.0 {
            return None;
        }
        p = ray.origin + ray.direction * t;
        h = ground.0.height_at(p.x, -p.z).max(0.0);
    }
    Some((p.x, -p.z))
}

/// The character nearest to a logic point within `radius`, preferring `faction`'s.
fn character_near(m: &CampaignModel, x: f32, z: f32, radius: f32, prefer: Option<FactionId>) -> Option<CharacterId> {
    let mut best: Option<(bool, f32, CharacterId)> = None;
    for c in m.world.characters.values() {
        if c.position.0.raw() == 0 && c.position.1.raw() == 0 {
            continue;
        }
        let d = ((c.position.0.to_f32() - x).powi(2) + (c.position.1.to_f32() - z).powi(2)).sqrt();
        if d > radius {
            continue;
        }
        let key = (Some(c.faction) != prefer, d, c.id);
        if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
            best = Some(key);
        }
    }
    best.map(|b| b.2)
}

/// What a left click on the map selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MapPick {
    /// A character standing on the map.
    character: Option<CharacterId>,
    /// A settlement.
    region: Option<RegionId>,
    /// A fort (`REGION/FORT_ARRAY`), reported as the region holding it, because
    /// `CampaignSelection::Fort` and [`CampaignSim::selected_fort`] both carry a region.
    /// PROVISIONAL: a region with two forts cannot be told apart by a click.
    fort: Option<RegionId>,
}

/// What a left click on the map selects: a character standing on the map, else a fort, else a
/// settlement (any owner; the HUD shows its panels). Characters inside a settlement (`garrisoned_in`)
/// and ministers (government posts, standing at their capital) are not pickable: in the original
/// neither is drawn on the map, and a click on the town opens the settlement (INFERRED). On the
/// install, ministers blocked all 23 capitals of the Europe start position until they were skipped,
/// and then a general standing on each capital without `garrisoned_in` did: a character standing on
/// a settlement's own position is inside it (INFERRED; see [`on_a_settlement`]). Without this, a
/// settlement's garrison commander or governor, standing at the settlement's own position, took every
/// click and no settlement panel ever opened.
///
/// **Forts come second**, after a character and before the settlement. INFERRED from the shipped
/// loc: a fort is a map object armies are moved into (`army_fort_Tooltip_12006e` = "Build fort ||
/// Building a fort requires a general and you must be within your own region.",
/// `campaign_map_tooltips_tooltip_line_armyplayer_fort` = "Right click to enter fort"), so a fort
/// standing near a settlement must not be shadowed by it. Nothing can confirm it: no shipped file
/// has a fort (`World::forts` is empty in all of them), so this branch never runs today.
fn map_pick(m: &CampaignModel, x: f32, z: f32, human: Option<FactionId>) -> MapPick {
    let mut best: Option<(bool, f32, CharacterId)> = None;
    for c in m.world.characters.values() {
        if c.garrisoned_in.is_some()
            || c.kind == CharacterKind::Minister
            || (c.position.0.raw() == 0 && c.position.1.raw() == 0)
            || on_a_settlement(m, c.position)
        {
            continue;
        }
        let d = ((c.position.0.to_f32() - x).powi(2) + (c.position.1.to_f32() - z).powi(2)).sqrt();
        if d > PICK_RADIUS {
            continue;
        }
        let key = (Some(c.faction) != human, d, c.id);
        if best.is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
            best = Some(key);
        }
    }
    if let Some(b) = best {
        return MapPick { character: Some(b.2), region: None, fort: None };
    }
    if let Some(f) = fort_near(m, x, z) {
        return MapPick { character: None, region: None, fort: Some(f) };
    }
    MapPick { character: None, region: settlement_near(m, x, z, PICK_RADIUS), fort: None }
}

/// The region of the fort nearest to a logic point, if one is within [`FORT_PICK_RADIUS`]. `None`
/// when the file has no forts (every shipped file) or a fort's record gave no position.
fn fort_near(m: &CampaignModel, x: f32, z: f32) -> Option<RegionId> {
    m.world
        .forts
        .values()
        .filter_map(|f| {
            let (px, pz) = f.position?;
            Some((((px.to_f32() - x).powi(2) + (pz.to_f32() - z).powi(2)).sqrt(), f.region))
        })
        .filter(|(d, _)| *d <= FORT_PICK_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, r)| r)
}

/// Picking radius for a fort, in map units. PLACEHOLDER (presentation only): the original's own
/// clickable size for a fort is not traced, and no shipped file has a fort to measure against.
const FORT_PICK_RADIUS: f32 = 1.5;

/// Whether a logic position is a settlement's own position (within `ON_SETTLEMENT`).
fn on_a_settlement(m: &CampaignModel, p: (Fixed20, Fixed20)) -> bool {
    let (x, z) = (p.0.to_f32(), p.1.to_f32());
    m.world.regions.values().any(|r| {
        let (sx, sz) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        (sx - x).abs() <= ON_SETTLEMENT && (sz - z).abs() <= ON_SETTLEMENT
    })
}

/// How close to a settlement's position counts as standing on it, in map units. PLACEHOLDER: the
/// install's capital generals stand on it exactly; the original's own test is not traced.
const ON_SETTLEMENT: f32 = 0.05;

/// The region whose settlement is within `radius` of a logic point.
fn settlement_near(m: &CampaignModel, x: f32, z: f32, radius: f32) -> Option<RegionId> {
    m.world
        .regions
        .values()
        .map(|r| (((r.settlement.position.0.to_f32() - x).powi(2) + (r.settlement.position.1.to_f32() - z).powi(2)).sqrt(), r.id))
        .filter(|(d, _)| *d <= radius)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// Picking radius in map units. PLACEHOLDER (presentation only).
const PICK_RADIUS: f32 = 2.0;

/// Harness `--campaign-demo-embark`: a fleet of the human faction sails into one of its ports, an
/// army walks there and boards it, the fleet sails to another shore and lands it.
#[derive(Debug, Clone)]
pub struct EmbarkDemo {
    navy: ForceId,
    port: (Fixed20, Fixed20),
    army: ForceId,
    target: Option<(Fixed20, Fixed20)>,
    stage: u8,
    turns: u32,
}

fn fdist(a: (Fixed20, Fixed20), b: (Fixed20, Fixed20)) -> f32 {
    (a.0.to_f32() - b.0.to_f32()).hypot(a.1.to_f32() - b.1.to_f32())
}

/// Picks the (fleet, port, army) with the shortest fleet + army distances, the army standing on the
/// port's land (same land component as a boarding point).
pub fn embark_demo_pick(m: &CampaignModel, faction: FactionId) -> Option<EmbarkDemo> {
    use ntw_sim::campaign::embark::embark_points;
    use ntw_sim::campaign::polypath::Mover;
    let pm = m.terrain.as_ref()?.0.poly.as_ref()?;
    let comp = |p: (Fixed20, Fixed20)| pm.locate(p.0.to_f32(), p.1.to_f32(), Mover::Land, 2).map(|q| pm.component[0][q]);
    let mut best: Option<(f32, EmbarkDemo)> = None;
    for r in m.world.regions.values().filter(|r| r.owner == faction) {
        for port in r.slots.iter().filter(|s| s.port).filter_map(|s| s.position) {
            let Some(&(q, _)) = embark_points(pm, (port.0.to_f32(), port.1.to_f32())).first() else { continue };
            for n in m.world.forces.values().filter(|f| f.faction == faction && f.is_navy && f.commander.is_some()) {
                for a in m.world.forces.values().filter(|f| f.faction == faction && !f.is_navy && f.commander.is_some()) {
                    let (Some(np), Some(ap)) = (m.force_position(n.id), m.force_position(a.id)) else { continue };
                    if comp(ap) != Some(pm.component[0][q]) {
                        continue;
                    }
                    let d = fdist(np, port) + fdist(ap, port);
                    if best.as_ref().is_none_or(|b| d < b.0) {
                        best = Some((d, EmbarkDemo { navy: n.id, port, army: a.id, target: None, stage: 0, turns: 0 }));
                    }
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// One harness step of the embark demo; `true` when it is over.
pub fn embark_demo_step(sim: &mut CampaignSim, d: &mut EmbarkDemo) -> bool {
    d.turns += 1;
    if d.turns > 30 {
        warn!("Embark demo: gave up at stage {}", d.stage);
        return true;
    }
    let pos = |sim: &CampaignSim, f: ForceId| sim.model().force_position(f);
    match d.stage {
        0 => {
            if pos(sim, d.navy).is_some_and(|p| fdist(p, d.port) < 1e-3) {
                d.stage = 1;
                return false;
            }
            sim.command(CampaignCommand::MoveForce { force: d.navy, to: d.port });
            info!("Embark demo: fleet {} -> port: {}", d.navy.raw(), sim.last_message);
            if !pos(sim, d.navy).is_some_and(|p| fdist(p, d.port) < 1e-3) {
                sim.end_turn();
            }
        }
        1 => {
            sim.command(CampaignCommand::Embark { force: d.army, navy: d.navy });
            info!("Embark demo: army {} boards fleet {}: {}", d.army.raw(), d.navy.raw(), sim.last_message);
            if sim.model().carrier_of(d.army).is_some() {
                d.stage = 2;
            }
            sim.end_turn();
        }
        2 => {
            let m = sim.model();
            let fleet = m.force_position(d.navy);
            d.target = fleet.and_then(|fleet| {
                let mut rs: Vec<_> = m.world.regions.values().filter(|r| r.slots.iter().any(|s| s.port)).collect();
                rs.sort_by(|a, b| fdist(a.settlement.position, fleet).total_cmp(&fdist(b.settlement.position, fleet)));
                rs.into_iter()
                    .filter(|r| (30.0..120.0).contains(&fdist(r.settlement.position, fleet)))
                    .find(|r| m.plan_transport(d.army, r.settlement.position).is_some_and(|p| p.landing().is_some()))
                    .map(|r| r.settlement.position)
            });
            drop(m);
            info!("Embark demo: landing target {:?}", d.target.map(|t| (t.0.to_f32(), t.1.to_f32())));
            d.stage = if d.target.is_some() { 3 } else { 4 };
        }
        3 => {
            let to = d.target.expect("stage 3 has a target");
            sim.command(CampaignCommand::MoveForce { force: d.army, to });
            info!("Embark demo: army {} sails to land: {}", d.army.raw(), sim.last_message);
            if sim.model().carrier_of(d.army).is_none() {
                let m = sim.model();
                let p = m.force_position(d.army).map(|p| (p.0.to_f32(), p.1.to_f32()));
                let c = m.world.forces.get(&d.army).and_then(|f| f.commander);
                drop(m);
                info!("Embark demo: landed at {p:?}");
                sim.selected = c;
                d.stage = 4;
                return true;
            }
            sim.end_turn();
        }
        _ => return true,
    }
    false
}

/// What a right click on (x, z) means for the selected character.
pub fn order_for(m: &CampaignModel, me: CharacterId, x: f32, z: f32) -> Option<CampaignCommand> {
    let ch = m.world.characters.get(&me)?;
    let to = (Fixed20::from_f64(x as f64), Fixed20::from_f64(z as f64));
    let Some(force) = m.force_of(me) else {
        return Some(CampaignCommand::MoveCharacter { character: me, to });
    };
    if let Some(other) = character_near(m, x, z, PICK_RADIUS, None).filter(|c| *c != me)
        && let Some(target) = m.force_of(other)
    {
        let t = &m.world.forces[&target];
        let mine_is_navy = m.world.forces.get(&force).is_some_and(|f| f.is_navy);
        return Some(if t.faction == ch.faction && t.is_navy && !mine_is_navy {
            // An army ordered onto its own fleet boards it (CCQ_EMBARK_NAVY).
            CampaignCommand::Embark { force, navy: target }
        } else if t.faction == ch.faction {
            CampaignCommand::MergeForces { force, into: target }
        } else {
            CampaignCommand::AttackForce { force, target }
        });
    }
    if let Some(region) = settlement_near(m, x, z, PICK_RADIUS) {
        return Some(CampaignCommand::EnterSettlement { force, region });
    }
    Some(CampaignCommand::MoveForce { force, to })
}

/// Mouse and keyboard input on the map.
#[allow(clippy::too_many_arguments)]
pub fn input(
    sim: Option<NonSendMut<CampaignSim>>,
    ground: Option<Res<CampaignGround>>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<CampaignCamera>>,
    hud: Option<NonSend<super::hud::CampaignHud>>,
    mut right_down: Local<Option<Vec2>>,
) {
    let (Some(mut sim), Some(ground)) = (sim, ground) else { return };
    if keys.just_pressed(KeyCode::Enter) {
        sim.end_turn();
    }
    if keys.just_pressed(KeyCode::F5) {
        quick_save(&mut sim);
    }
    // Clicks on the HUD belong to the HUD.
    if let (Some(h), Some(c)) = (&hud, window.cursor_position())
        && h.covers(c)
    {
        return;
    }
    let (camera, cam_tf) = *camera;
    if buttons.just_pressed(MouseButton::Right) {
        *right_down = window.cursor_position();
    }
    let Some((x, z)) = cursor_ground(&window, camera, cam_tf, &ground) else { return };
    if buttons.just_pressed(MouseButton::Left) {
        let human = sim.human_id();
        let pick = map_pick(&sim.model(), x, z, human);
        sim.selected = pick.character;
        sim.selected_region = pick.region;
        // A fort of the region that holds it (PROVISIONAL: two forts in one region cannot be told
        // apart; see `MapPick`).
        sim.selected_fort = pick.fort;
        sim.generation += 1;
    }
    if buttons.just_released(MouseButton::Right) {
        // A right drag pans the camera; only a click (little movement) is an order.
        let click = matches!((*right_down, window.cursor_position()), (Some(a), Some(b)) if a.distance(b) < 5.0);
        if click {
            right_click(&mut sim, x, z);
        }
    }
}

/// Gives the selected character the order a right click at (x, z) means.
pub fn right_click(sim: &mut CampaignSim, x: f32, z: f32) {
    let Some(me) = sim.selected else { return };
    let human = sim.human_id();
    let cmd = {
        let m = sim.model();
        if m.world.characters.get(&me).map(|c| c.faction) != human {
            return;
        }
        order_for(&m, me, x, z)
    };
    if let Some(cmd) = cmd {
        sim.command(cmd);
    }
}

/// What [`preview`] caches between frames: the cursor cell the path was planned for, the model
/// generation it was planned at, and the path itself.
type PreviewCache = Option<((i32, i32), u64, super::arrows::ArrowPath)>;

/// Draws the selection ring and, while hovering, publishes the planned path for
/// [`super::arrows::draw`], which draws the original's textured arrows along it (the two colours:
/// the part still reachable this turn and the part beyond). The ring stays a gizmo; the original
/// draws its own selection marker (UNKNOWN — `analysis/fidelity/UI_FIDELITY.md` §7).
#[allow(clippy::too_many_arguments)]
pub fn preview(
    sim: Option<NonSendMut<CampaignSim>>,
    ground: Option<Res<CampaignGround>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<CampaignCamera>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    mut gizmos: Gizmos,
    mut commands: Commands,
    current: Option<Res<super::arrows::ArrowPath>>,
    mut cache: Local<PreviewCache>,
) {
    let (Some(mut sim), Some(ground)) = (sim, ground) else { return };
    // No path to show: withdraw the published one, so the arrows go away as the gizmo line did.
    let withdraw = |commands: &mut Commands| {
        if current.is_some() {
            commands.remove_resource::<super::arrows::ArrowPath>();
        }
    };
    let Some(me) = sim.selected else {
        withdraw(&mut commands);
        return;
    };
    let Some((cx, cz)) = sim.model().world.characters.get(&me).map(|c| (c.position.0.to_f32(), c.position.1.to_f32())) else {
        sim.selected = None;
        withdraw(&mut commands);
        return;
    };
    let ring = ground_pos(&ground, cx, cz, 0.15);
    gizmos.circle(Isometry3d::new(ring, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), 1.2, Color::srgb(1.0, 0.9, 0.2));
    let (camera, cam_tf) = *camera;
    // Harness runs ignore the real cursor (repeatable captures).
    let live = harness.is_none();
    let target = sim.demo_target.or_else(|| if live { cursor_ground(&window, camera, cam_tf, &ground) } else { None });
    let Some((x, z)) = target else {
        withdraw(&mut commands);
        return;
    };
    let key = ((x * 2.0) as i32, (z * 2.0) as i32);
    let fresh = !matches!(&*cache, Some((k, g, _)) if *k == key && *g == sim.generation);
    if fresh {
        let to = (Fixed20::from_f64(x as f64), Fixed20::from_f64(z as f64));
        let plan = sim.model().plan_path(me, to);
        *cache = plan.map(|p| {
            let path = super::arrows::ArrowPath {
                generation: sim.generation,
                points: p.path.points.clone(),
                reachable: p.reachable,
            };
            (key, sim.generation, path)
        });
    }
    // Hand the path to the arrow renderer, which owns the movement arrow from here on.
    match &*cache {
        Some((_, _, path)) if current.as_deref() != Some(path) => commands.insert_resource(path.clone()),
        Some(_) => {}
        None => withdraw(&mut commands),
    }
}

/// `F5`: writes the campaign to `<NapoleonRust user folder>\save_games\quick_save.save` (never the
/// original game's folder).
pub fn quick_save(sim: &mut CampaignSim) {
    let Some(dir) = crate::config::user_dir().map(|d| d.join("save_games")) else { return };
    // The scripts write their `save_value` slots during `SavingGame` (CAMPAIGN_DATA.md §4).
    let (values, report) = sim.host.save_values();
    for e in report.errors {
        warn!("Campaign script SavingGame: {e}");
    }
    let values = script_values_out(&values);
    // The scripts' restricted lists (EPISODIC_RESTRICTIONS): without them a loaded game offers
    // the tutorial and Peninsular buildings again.
    let restrictions = {
        let st = sim.host.state();
        ntw_campaign::script_values::ScriptRestrictions {
            buildings: st.model.world.restricted_buildings.iter().cloned().collect(),
            units: st.restricted_units.iter().cloned().collect(),
        }
    };
    let result = (|| -> Result<std::path::PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let source = ntw_formats::esf::EsfFile::from_bytes(&sim.source)?;
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32);
        let mut tree = ntw_campaign::save::write_save_named(&source, &sim.model(), &sim.human, ts, Some(&values), sim.names.as_deref())?;
        ntw_campaign::script_values::write_restrictions(&mut tree, &restrictions);
        ntw_campaign::header_map::update_maps(&mut tree, &sim.model(), &sim.human, &sim.pictures);
        let bytes = tree.to_bytes()?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("quick_save.save");
        std::fs::write(&path, bytes)?;
        Ok(path)
    })();
    sim.last_message = match result {
        Ok(p) => format!("Saved {} to {}", sim.campaign, p.display()),
        Err(e) => format!("Save failed: {e}"),
    };
    info!("Campaign: {}", sim.last_message);
}

/// A save's `save_value` slots as script values (`load_value` returns integers as numbers).
pub fn script_values_in(values: &[ntw_campaign::script_values::ScriptSaveValue]) -> Vec<ntw_script::ScriptValue> {
    use ntw_campaign::script_values::ScriptSaveValue as S;
    values
        .iter()
        .map(|v| match *v {
            S::Bool(b) => ntw_script::ScriptValue::Bool(b),
            S::Int(i) => ntw_script::ScriptValue::Number(i as f32),
        })
        .collect()
}

/// The scripts' saved values as the original stores them: booleans as bool, everything else as
/// `lua_tointeger` would give it (numbers truncated, numeric strings parsed, other values 0).
pub fn script_values_out(values: &[ntw_script::ScriptValue]) -> Vec<ntw_campaign::script_values::ScriptSaveValue> {
    use ntw_campaign::script_values::ScriptSaveValue as S;
    values
        .iter()
        .map(|v| match v {
            ntw_script::ScriptValue::Bool(b) => S::Bool(*b),
            ntw_script::ScriptValue::Number(n) => S::Int(*n as i32),
            ntw_script::ScriptValue::String(s) => S::Int(s.trim().parse::<f64>().map_or(0, |n| n as i32)),
            ntw_script::ScriptValue::Nil => S::Int(0),
        })
        .collect()
}

/// Removes the campaign when leaving the mode.
pub fn leave(world: &mut World) {
    world.remove_non_send::<CampaignSim>();
    world.remove_non_send::<super::hud::CampaignHud>();
    world.remove_resource::<MarkerAssets>();
    world.remove_resource::<CampaignGround>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A click on any settlement of the Europe start position opens that settlement, unless a
    /// character standing on the map (not inside a settlement) is nearer. Needs the install.
    #[test]
    fn a_click_on_a_settlement_selects_it() {
        let dir = crate::config::game_data_dir();
        let Ok(db) = ntw_data::GameDatabase::from_install(&dir) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let m = ntw_campaign::load_startpos_file(dir.join("campaigns").join("eur_napoleon").join("startpos.esf"), &db).unwrap();
        let human = m.faction_by_key("france").map(|f| f.id);
        let mut blocked = Vec::new();
        let mut checked = 0;
        for r in m.world.regions.values() {
            let (x, z) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
            if x == 0.0 && z == 0.0 {
                continue;
            }
            checked += 1;
            match map_pick(&m, x, z, human) {
                MapPick { character: None, region: Some(got), fort: None } if got == r.id => {}
                MapPick { character: Some(c), .. } => {
                    let ch = &m.world.characters[&c];
                    let d = ((ch.position.0.to_f32() - x).powi(2) + (ch.position.1.to_f32() - z).powi(2)).sqrt();
                    blocked.push(format!("{}: character {:?} {:?} garrisoned_in={:?} {d:.3} units away", r.key, c, ch.kind, ch.garrisoned_in));
                }
                other => blocked.push(format!("{}: {other:?}", r.key)),
            }
        }
        eprintln!("{checked} settlements checked, {} blocked", blocked.len());
        assert!(checked > 0);
        assert!(blocked.is_empty(), "settlement clicks taken by something else:\n{}", blocked.join("\n"));
    }
}
