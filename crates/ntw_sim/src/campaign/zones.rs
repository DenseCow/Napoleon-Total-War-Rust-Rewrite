//! The stored zones of control: each force commander's zone ([`zoc::zone_in`]), kept in the model
//! and rebuilt only at the events the original rebuilds it at (`analysis/campaign/PATHFINDING.md` §8,
//! CONFIRMED by disassembly):
//!
//! - **Loading** (`0x00B0DC10`, from the campaign constructor `0x008742C0`): the zones are not saved;
//!   every one is flooded again, faction by faction in turn order with the rebels last (the world's
//!   list, campaign +0xF5C +0x30, then the rebels), each character of the faction's list (+0x768)
//!   flooded and then added as an obstacle (`0x00B084F0`). So each flood sees only the obstacles added
//!   before it: of two neighbouring enemies, the later one's zone stops at the earlier one's, while
//!   the earlier one's covers the ground between them. The constructor runs it right after the world
//!   is read (`0x0090C4A0`) and before the later subsystems; the floods the world load would make
//!   wait for it (the model's +0xFC7 flag, cleared just before). Ours:
//!   [`CampaignModel::attach_terrain`], when the map is put into a loaded model, in that order.
//!   Nothing else builds the store: a read never does.
//! - **A character's own changes** (`0x00A285C0`, and `0x00A285E0` after a move): its move
//!   ([`CampaignModel::rebuild_moved_zones`], with the armies it carries), a force's commander or
//!   make-up changing (`0x008F9400`: the commander pick `0x008B8E50`, merges, transfers `0x008D47B0` /
//!   `0x008D5F10`; a split or new force `0x008F04C0`; boarding joins `0x008F0F20` / `0x008F17D0`; a
//!   commander's death `0x008DB3E0`), a promotion in the field (`0x008E1C20` / `0x008E2260`), a hire
//!   (`0x00A164C0`) and a recruited general placed (`0x00B67670`): [`CampaignModel::rebuild_zone`]
//!   and [`CampaignModel::rebuild_force_zones`].
//! - **Diplomacy** (`RefreshFactionPathGridObstacles` `0x00B795A0`: every live character of one
//!   faction): [`CampaignModel::refresh_faction_zones`], called by each rule in the exe's order
//!   (treaties and expulsion modules).
//!
//! Between those events a stored zone stays as it was flooded, wherever its character stands now:
//! the exe keeps the flood's layer on the character (+0x1C8) until one of them (an expelled army that
//! is not moved keeps its old zone, for one).
//!
//! Not ported: the script barriers' refresh (`0x00B64E30`, every faction; no barriers are modelled,
//! [`zoc::zone_in`]) and `0x0096CBE0`'s rebuild of characters with the +0x1B7 flag (UNKNOWN flag,
//! which `0x00B795A0` skips too; no modelled state sets it).
//!
//! The faction's character list (+0x768) is taken in character-id order: new characters get rising
//! ids, and the start position's `CHARACTER_ARRAY`s list their characters in rising id order
//! (CONFIRMED by data, `ntw_campaign` test `zoc_install.rs`). INFERRED: the loader fills +0x768 in
//! `CHARACTER_ARRAY` order and new characters are appended (not traced). It matters only for a
//! faction's own characters, whose cores bound each other's floods in that order. A flood's obstacles
//! are taken in force order, as the path search takes them.
//!
//! A commander without a stored zone (a force made by a path that floods none, or a terrain set
//! without [`CampaignModel::attach_terrain`]) is read as his bare-map flood ([`zoc::reach`]), logged
//! once: our own fallback.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::ids::{CharacterId, FactionId, ForceId};
use super::polypath::{Mover, PolyMap};
use super::world::{CampaignModel, MilitaryForce, Stance, Terrain};
use super::zoc::{self, Obstacle};
use crate::fixed::Fixed20;

/// Each commander's stored zone: the static polygons of the zone, sorted ([`zoc::zone_in`]).
pub type StoredZones = BTreeMap<CharacterId, Arc<[u32]>>;

/// The model's stored zones (not saved: a load floods them again, as `0x00B0DC10` does), with the map
/// they were flooded on.
#[derive(Debug, Clone, Default)]
pub struct Zones(Option<Built>);

#[derive(Debug, Clone)]
struct Built {
    terrain: Terrain,
    zones: StoredZones,
}

/// A commander's obstacle as the zone rules see it.
#[derive(Debug, Clone, Copy)]
struct Commander {
    id: CharacterId,
    faction: FactionId,
    force: ForceId,
    pos: (Fixed20, Fixed20),
    navy: bool,
    garrisoned: bool,
}

impl Commander {
    fn pos_f32(&self) -> (f32, f32) {
        (self.pos.0.to_f32(), self.pos.1.to_f32())
    }
}

/// Set once the missing-zone fallback has been logged.
static MISSING_ZONE_LOGGED: AtomicBool = AtomicBool::new(false);

/// The bare-map flood: the fallback for a commander without a stored zone (module docs).
fn bare_zone(map: &PolyMap, c: &Commander) -> Arc<[u32]> {
    if !MISSING_ZONE_LOGGED.swap(true, Ordering::Relaxed) {
        log::warn!("campaign zones: commander {} has no stored zone of control; his bare-map flood is used (logged once)", c.id.raw());
    }
    zoc::reach(map, c.pos_f32(), if c.navy { Mover::Sea } else { Mover::Land }, zoc::zone_limit(c.navy, c.garrisoned)).into()
}

impl CampaignModel {
    fn poly_map(&self) -> Option<&PolyMap> {
        self.terrain.as_ref()?.0.poly.as_ref()
    }

    /// `f`'s commander as an obstacle, if it has one.
    fn commander_of_force(&self, f: &MilitaryForce) -> Option<Commander> {
        let ch = self.world.characters.get(&f.commander?)?;
        Some(Commander { id: ch.id, faction: f.faction, force: f.id, pos: ch.position, navy: f.is_navy, garrisoned: ch.garrisoned_in.is_some() })
    }

    /// `c` as a force commander's obstacle, if he commands a force.
    fn commander(&self, c: CharacterId) -> Option<Commander> {
        self.world.forces.values().find(|f| f.commander == Some(c)).and_then(|f| self.commander_of_force(f))
    }

    /// Every force commander, in force order.
    fn commanders(&self) -> impl Iterator<Item = Commander> + '_ {
        self.world.forces.values().filter_map(|f| self.commander_of_force(f))
    }

    /// Every force commander in character-id order (the faction lists' order, module docs).
    fn commanders_by_id(&self) -> Vec<Commander> {
        let mut out: Vec<Commander> = self.commanders().collect();
        out.sort_by_key(|c| c.id);
        out
    }

    /// The commanders `faction` sees as obstacles, leaving out `character` and its own force, each with
    /// whether `faction` is at war with it (`CampaignModel::obstacles_seen_by`'s rule).
    fn seen_commanders(&self, faction: FactionId, character: CharacterId) -> impl Iterator<Item = (Commander, bool)> + '_ {
        let own_force = self.force_of(character);
        self.commanders()
            .filter(move |c| Some(c.force) != own_force && c.id != character)
            .filter(move |c| self.sees(faction, c.pos_f32()) && super::agents::knows_character(self, faction, c.id))
            .map(move |c| {
                let at_war = c.faction != faction && self.world.stance(faction, c.faction) == Stance::War;
                (c, at_war)
            })
    }

    /// The obstacles `faction` sees (leaving out `character` and its force), each with its stored zone
    /// (or the logged bare-map fallback, module docs; an empty zone without a polygon map);
    /// `only_stored` leaves out an obstacle without a stored zone instead (the load pass: not added yet).
    fn obstacles_with_zones(&self, map: Option<&PolyMap>, faction: FactionId, character: CharacterId, zones: Option<&StoredZones>, only_stored: bool) -> Vec<Obstacle> {
        self.seen_commanders(faction, character)
            .filter_map(|(c, at_war)| {
                let zone = match (zones.and_then(|z| z.get(&c.id)), map) {
                    (Some(z), _) => Arc::clone(z),
                    (None, _) if only_stored => return None,
                    (None, Some(map)) => bare_zone(map, &c),
                    (None, None) => Arc::from([]),
                };
                Some(Obstacle { pos: c.pos_f32(), navy: c.navy, garrisoned: c.garrisoned, at_war, zone })
            })
            .collect()
    }

    /// Floods `c`'s zone against the obstacles his faction sees with their stored zones.
    fn flood_zone(&self, map: &PolyMap, c: &Commander, zones: &StoredZones, only_stored: bool) -> Arc<[u32]> {
        let others = self.obstacles_with_zones(Some(map), c.faction, c.id, Some(zones), only_stored);
        zoc::zone_in(map, c.pos_f32(), c.navy, c.garrisoned, &others).into()
    }

    /// The load pass `0x00B0DC10` (module docs): every commander flooded in turn order, the rebels
    /// last, each against those flooded before it.
    fn build_zones(&self, map: &PolyMap) -> StoredZones {
        let commanders = self.commanders_by_id();
        let mut zones = BTreeMap::new();
        for f in self.world.factions_in_turn_order() {
            for c in commanders.iter().filter(|c| c.faction == f) {
                let z = self.flood_zone(map, c, &zones, true);
                zones.insert(c.id, z);
            }
        }
        zones
    }

    /// Puts the campaign map's movement grid into a loaded model and floods every stored zone (the
    /// load pass, module docs).
    pub fn attach_terrain(&mut self, terrain: Terrain) {
        self.terrain = Some(terrain);
        self.build_stored_zones();
    }

    /// The load pass `0x00B0DC10` on the current map: every stored zone flooded again (none without a
    /// polygon map).
    pub fn build_stored_zones(&mut self) {
        let built = match (&self.terrain, self.poly_map()) {
            (Some(t), Some(map)) => Some(Built { terrain: t.clone(), zones: self.build_zones(map) }),
            _ => None,
        };
        self.zones = Zones(built);
    }

    /// The stored zones; `None` without a polygon map, or when they were not flooded for the current
    /// one ([`Self::attach_terrain`]).
    pub fn stored_zones(&self) -> Option<&StoredZones> {
        let b = self.zones.0.as_ref()?;
        (Some(&b.terrain) == self.terrain.as_ref()).then_some(&b.zones)
    }

    fn stored_zones_mut(&mut self) -> Option<&mut StoredZones> {
        let b = self.zones.0.as_mut()?;
        (Some(&b.terrain) == self.terrain.as_ref()).then_some(&mut b.zones)
    }

    /// The stored zone of `character`, if he has one.
    pub fn stored_zone(&self, character: CharacterId) -> Option<Arc<[u32]>> {
        self.stored_zones()?.get(&character).cloned()
    }

    /// The obstacles `faction` sees, leaving out `character` and its force, with their stored zones
    /// (the path search's set, [`Self::obstacles_seen_by`]). Empty zones without a polygon map.
    pub(crate) fn obstacles_with_stored_zones(&self, faction: FactionId, character: CharacterId) -> Vec<Obstacle> {
        self.obstacles_with_zones(self.poly_map(), faction, character, self.stored_zones(), false)
    }

    /// `0x00A285C0` / `0x00A285E0`: floods `character`'s zone again against the stored zones of the
    /// obstacles his faction sees, and stores it; a character who commands no force loses his entry.
    /// Nothing without zones flooded for the current map ([`Self::attach_terrain`]).
    pub(crate) fn rebuild_zone(&mut self, character: CharacterId) {
        let new = match (self.poly_map(), self.stored_zones()) {
            (Some(map), Some(zones)) => self.commander(character).map(|c| self.flood_zone(map, &c, zones, false)),
            _ => return,
        };
        let Some(zones) = self.stored_zones_mut() else { return };
        match new {
            Some(z) => {
                zones.insert(character, z);
            }
            None => {
                zones.remove(&character);
            }
        }
    }

    /// `0x008F9400`: a force's commander (and the commanders of the armies a navy carries, or of the
    /// navy carrying this army) rebuilt after the force changed.
    pub(crate) fn rebuild_force_zones(&mut self, force: ForceId) {
        let Some(c) = self.world.forces.get(&force).and_then(|f| f.commander) else { return };
        self.rebuild_zone(c);
        let mut linked: Vec<ForceId> = self.passengers_of(force);
        linked.extend(self.carrier_of(force));
        for f in linked {
            if let Some(c) = self.world.forces.get(&f).and_then(|f| f.commander) {
                self.rebuild_zone(c);
            }
        }
    }

    /// `0x00A285E0` after `character` moved ([`Self::move_character`]): his force's zones, with the
    /// armies a navy carries; a character without a force loses his entry.
    pub(crate) fn rebuild_moved_zones(&mut self, character: CharacterId) {
        match self.force_of(character) {
            Some(force) => self.rebuild_force_zones(force),
            None => self.rebuild_zone(character),
        }
    }

    /// `RefreshFactionPathGridObstacles` `0x00B795A0`: every commander of `faction` rebuilt, in its
    /// list's order; entries of characters who no longer command a force are dropped.
    pub(crate) fn refresh_faction_zones(&mut self, faction: FactionId) {
        if self.stored_zones().is_none() {
            return;
        }
        let commanders = self.commanders_by_id();
        if let Some(zones) = self.stored_zones_mut() {
            zones.retain(|id, _| commanders.binary_search_by_key(id, |c| c.id).is_ok());
        }
        for c in commanders.iter().filter(|c| c.faction == faction) {
            self.rebuild_zone(c.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::commands::CampaignCommand;
    use super::super::ids::{RegionId, UnitId};
    use super::super::pathing::PathGrid;
    use super::super::world::{Character, CharacterKind};
    use super::super::zoc::{tests::open, zone_in, ARMY_ZONE};
    use super::*;

    const A: FactionId = FactionId(1);
    const B: FactionId = FactionId(2);
    /// A's general (force 1000) and B's colonel (force 1001); turn order B, A.
    const GA: CharacterId = CharacterId(100);
    const GB: CharacterId = CharacterId(101);

    fn at(x: i32, z: i32) -> (Fixed20, Fixed20) {
        (Fixed20::from_int(x), Fixed20::from_int(z))
    }

    fn open_terrain() -> Terrain {
        let mut grid = PathGrid::new((0.0, 0.0), 2.0, 40, 20);
        grid.poly = Some(open(40, 20));
        Terrain(Arc::new(grid))
    }

    /// The made-up test world: A's general at (41, 21), B's colonel 8 units east at (49, 21), at war
    /// when `war`; not yet on a map.
    fn world(war: bool) -> CampaignModel {
        let mut m = super::super::tests::test_model();
        m.world.characters.get_mut(&GA).unwrap().position = at(41, 21);
        m.world.characters.get_mut(&GB).unwrap().position = at(49, 21);
        if war {
            m.world.set_stance(A, B, Stance::War).unwrap();
        }
        m
    }

    /// [`world`] loaded onto an open 80 x 40 map.
    fn model(war: bool) -> CampaignModel {
        let mut m = world(war);
        m.attach_terrain(open_terrain());
        m
    }

    fn map(m: &CampaignModel) -> &PolyMap {
        m.poly_map().unwrap()
    }

    fn bare(m: &CampaignModel, p: (f32, f32)) -> Vec<u32> {
        zoc::reach(map(m), p, Mover::Land, ARMY_ZONE)
    }

    /// `c`'s zone flooded now against the stored zones of what his faction sees.
    fn fresh(m: &CampaignModel, c: CharacterId) -> Vec<u32> {
        let k = m.commander(c).expect("a commander");
        zone_in(map(m), k.pos_f32(), k.navy, k.garrisoned, &m.obstacles_with_stored_zones(k.faction, c))
    }

    fn zone(m: &CampaignModel, c: CharacterId) -> Vec<u32> {
        m.stored_zone(c).expect("a stored zone").to_vec()
    }

    fn holds(m: &CampaignModel, z: &[u32], p: (f32, f32)) -> bool {
        z.binary_search(&(map(m).polygon_at(p.0, p.1).unwrap() as u32)).is_ok()
    }

    fn obstacle(p: (f32, f32), zone: Vec<u32>) -> Obstacle {
        Obstacle { pos: p, navy: false, garrisoned: false, at_war: true, zone: zone.into() }
    }

    /// Empties `c`'s stored zone, so only a flood puts it back.
    fn spoil(m: &mut CampaignModel, c: CharacterId) {
        m.stored_zones_mut().unwrap().insert(c, Arc::from([]));
    }

    /// `c`'s stored zone was flooded again (it is not the spoiled one) and is current.
    fn assert_flooded(m: &CampaignModel, c: CharacterId) {
        let z = zone(m, c);
        assert!(!z.is_empty(), "{c:?} not flooded again");
        assert_eq!(z, fresh(m, c), "{c:?}");
    }

    #[test]
    fn the_load_pass_floods_in_turn_order_each_against_those_before() {
        let m = model(true);
        // B moves first: flooded before A is added, its zone is the bare flood; A's stops at it.
        assert_eq!(zone(&m, GB), bare(&m, (49.0, 21.0)));
        let a = zone(&m, GA);
        assert_eq!(a, zone_in(map(&m), (41.0, 21.0), false, false, &[obstacle((49.0, 21.0), zone(&m, GB))]));
        assert!(a.len() < bare(&m, (41.0, 21.0)).len());
        // Not saved: a loaded copy floods the same zones again when its map is attached.
        let mut loaded = m.clone();
        loaded.zones = Zones::default();
        assert!(loaded == m);
        assert_eq!(loaded.stored_zones(), None, "a read builds nothing");
        loaded.build_stored_zones();
        assert_eq!(loaded.stored_zones(), m.stored_zones());
    }

    #[test]
    fn the_zones_after_a_load_and_a_war_do_not_depend_on_a_preview() {
        let run = |preview: bool| {
            let mut m = model(false);
            if preview {
                // The cursor's path preview reads the zones.
                let _ = m.obstacles_seen_by(A, GA);
                let _ = m.plan_path(GA, at(60, 21));
            }
            m.declare_war_rules(A, B).unwrap();
            m.stored_zones().cloned()
        };
        let plain = run(false);
        assert!(plain.as_ref().is_some_and(|z| z.len() == 2));
        assert_eq!(run(true), plain);
    }

    #[test]
    fn neighbouring_enemies_leave_no_gap_between_their_zones() {
        let m = model(true);
        let mid = (45.0, 21.0);
        // Flooding each zone against the other's at search time would leave the ground between them
        // in neither zone.
        let a_cut = zone_in(map(&m), (41.0, 21.0), false, false, &[obstacle((49.0, 21.0), bare(&m, (49.0, 21.0)))]);
        let b_cut = zone_in(map(&m), (49.0, 21.0), false, false, &[obstacle((41.0, 21.0), bare(&m, (41.0, 21.0)))]);
        assert!(!holds(&m, &a_cut, mid) && !holds(&m, &b_cut, mid));
        // The stored zones cover it (B's, flooded first), and the search reads them.
        assert!(holds(&m, &zone(&m, GB), mid));
        let seen = m.obstacles_seen_by(FactionId(3), CharacterId(999));
        assert_eq!(seen.len(), 2);
        for o in &seen {
            let c = if o.pos.0 < 45.0 { GA } else { GB };
            assert_eq!(o.zone.as_ref(), zone(&m, c).as_slice());
        }
    }

    #[test]
    fn a_move_floods_the_movers_zone_only() {
        let mut m = model(true);
        let b = zone(&m, GB);
        m.move_character(GA, at(20, 21));
        m.rebuild_moved_zones(GA);
        assert_eq!(zone(&m, GA), bare(&m, (20.0, 21.0)));
        // B's zone is not flooded again: it is the same as before.
        assert_eq!(zone(&m, GB), b);
    }

    #[test]
    fn a_walk_floods_the_zone_after_the_sight() {
        let mut m = model(true);
        spoil(&mut m, GA);
        m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: at(36, 21) }).unwrap();
        assert_ne!(m.world.characters[&GA].position, at(41, 21));
        assert_flooded(&m, GA);
    }

    #[test]
    fn a_stored_zone_stays_until_an_event_floods_it() {
        let mut m = model(true);
        let b = zone(&m, GB);
        // Moved with no event after it (an expelled army that is not placed elsewhere keeps its zone too).
        m.world.characters.get_mut(&GB).unwrap().position = at(60, 21);
        assert_eq!(zone(&m, GB), b);
        let seen = m.obstacles_seen_by(A, GA);
        assert_eq!(seen[0].zone.as_ref(), b.as_slice());
        // A flood takes the new place.
        m.rebuild_zone(GB);
        assert_eq!(zone(&m, GB), bare(&m, (60.0, 21.0)));
    }

    #[test]
    fn a_war_floods_the_target_then_the_declarer_and_a_peace_both_again() {
        let mut m = model(false);
        let mid = (45.0, 21.0);
        // At peace only the cores cut: both zones reach the ground between them.
        let before_a = zone(&m, GA);
        assert!(holds(&m, &before_a, mid) && holds(&m, &zone(&m, GB), mid));
        m.declare_war_rules(A, B).unwrap();
        // B first, against A's peace-time zone; then A against B's new one.
        assert_eq!(zone(&m, GB), zone_in(map(&m), (49.0, 21.0), false, false, &[obstacle((41.0, 21.0), before_a)]));
        assert_eq!(zone(&m, GA), fresh(&m, GA));
        assert!(!holds(&m, &zone(&m, GB), mid));
        assert!(holds(&m, &zone(&m, GA), mid), "A's zone, flooded last, covers the ground between");
        m.make_peace_rules(A, B).unwrap();
        assert!(holds(&m, &zone(&m, GA), mid) && holds(&m, &zone(&m, GB), mid));
    }

    #[test]
    fn a_cancelled_access_floods_only_the_target_again() {
        let mut m = model(false);
        let (a, b) = (zone(&m, GA), zone(&m, GB));
        // Moved without an event: only a flood notices.
        m.world.characters.get_mut(&GA).unwrap().position = at(20, 21);
        m.world.characters.get_mut(&GB).unwrap().position = at(60, 21);
        // The rule alone floods nothing (`0x00B67BD0`).
        m.cancel_military_access(A, B);
        assert_eq!((zone(&m, GA), zone(&m, GB)), (a.clone(), b));
        // The command floods the target's zones (`0x00B67B20`), not the canceller's.
        m.apply_diplomatic_action(A, B, super::super::treaties::DiplomaticAction::CancelMilitaryAccess).unwrap();
        assert_eq!(zone(&m, GB), bare(&m, (60.0, 21.0)));
        assert_eq!(zone(&m, GA), a);
    }

    #[test]
    fn a_granted_access_floods_both_factions() {
        let mut m = model(false);
        spoil(&mut m, GA);
        spoil(&mut m, GB);
        m.grant_military_access(A, B, 4).unwrap();
        assert_flooded(&m, GA);
        assert_flooded(&m, GB);
    }

    #[test]
    fn an_access_running_out_floods_both_factions() {
        let mut m = model(false);
        m.grant_military_access(A, B, 1).unwrap();
        spoil(&mut m, GA);
        spoil(&mut m, GB);
        m.diplomacy_round_end(A);
        assert_eq!(m.world.relationships[&(A, B)].military_access_turns, 0);
        assert_flooded(&m, GA);
        assert_flooded(&m, GB);
    }

    #[test]
    fn a_protectorate_floods_the_patron() {
        let mut m = model(false);
        spoil(&mut m, GA);
        m.make_protectorate(B, A).unwrap();
        assert_flooded(&m, GA);
    }

    #[test]
    fn a_capture_floods_the_taker_once_inside() {
        let mut m = model(true);
        spoil(&mut m, GA);
        assert!(m.take_settlement(RegionId(11), A, Some(ForceId(1000)), &mut Vec::new()));
        assert_eq!(m.world.characters[&GA].garrisoned_in, Some(RegionId(11)));
        assert_flooded(&m, GA);
    }

    #[test]
    fn a_new_commander_gets_a_zone_and_the_dead_one_loses_his() {
        let mut m = model(true);
        assert!(m.stored_zones().unwrap().contains_key(&GB));
        m.character_dies(GB);
        let next = m.world.forces[&ForceId(1001)].commander.expect("a successor");
        assert!(!m.stored_zones().unwrap().contains_key(&GB));
        assert_eq!(zone(&m, next), fresh(&m, next));
    }

    /// A second army of A's, commanded by character 103 at (40, 25), with one unit.
    fn second_army(m: &mut CampaignModel) -> CharacterId {
        let c = CharacterId(103);
        let mut ch = m.world.characters[&GA].clone();
        ch.id = c;
        ch.position = at(40, 25);
        m.world.characters.insert(c, ch);
        let mut unit = m.world.forces[&ForceId(1000)].units[0].clone();
        unit.id = UnitId(77);
        unit.character = None;
        m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(c), units: vec![unit], is_navy: false });
        c
    }

    #[test]
    fn a_merge_floods_the_target_again_and_drops_the_emptied_forces_commander() {
        let mut m = world(true);
        let c = second_army(&mut m);
        m.attach_terrain(open_terrain());
        assert!(m.stored_zones().unwrap().contains_key(&c));
        m.merge_units(ForceId(1002), ForceId(1000), &mut Vec::new());
        assert!(!m.world.forces.contains_key(&ForceId(1002)));
        assert!(!m.stored_zones().unwrap().contains_key(&c));
        assert_eq!(zone(&m, GA), fresh(&m, GA));
    }

    #[test]
    fn a_transfer_floods_both_forces() {
        let mut m = world(true);
        let c = second_army(&mut m);
        let mut extra = m.world.forces[&ForceId(1002)].units[0].clone();
        extra.id = UnitId(78);
        m.world.forces.get_mut(&ForceId(1002)).unwrap().units.push(extra);
        m.attach_terrain(open_terrain());
        // The target has room for one unit only: the source keeps the other and its commander.
        let max = m.max_units(false);
        let filler = m.world.forces[&ForceId(1000)].units[0].clone();
        let units = &mut m.world.forces.get_mut(&ForceId(1000)).unwrap().units;
        while units.len() < max - 1 {
            let mut u = filler.clone();
            u.id = UnitId(200 + units.len() as i32);
            u.character = None;
            units.push(u);
        }
        spoil(&mut m, GA);
        spoil(&mut m, c);
        m.merge_units(ForceId(1002), ForceId(1000), &mut Vec::new());
        assert_eq!(m.world.forces[&ForceId(1002)].units.len(), 1);
        assert_flooded(&m, GA);
        assert_flooded(&m, c);
    }

    #[test]
    fn a_garrison_floods_the_zone_again_with_its_bonus() {
        let mut m = model(false);
        let open_zone = zone(&m, GA);
        m.world.characters.get_mut(&GA).unwrap().garrisoned_in = Some(RegionId(10));
        m.rebuild_zone(GA);
        let held = zone(&m, GA);
        assert!(held.len() > open_zone.len());
        assert_eq!(held, fresh(&m, GA));
    }

    #[test]
    fn a_hired_general_gets_a_zone() {
        let mut m = model(true);
        let mut rules = (*m.rules).clone();
        rules.general_units.insert(String::new(), "test_unit".into());
        m.rules = Arc::new(rules);
        let c = CharacterId(103);
        m.world.characters.insert(c, Character { id: c, faction: A, kind: CharacterKind::General, position: at(30, 21), movement_points: 30, max_movement_points: 30, base_movement_points: 30, garrisoned_in: None });
        let elapsed = m.calendar.turns_elapsed;
        m.world.faction_details.entry(A).or_default().general_pool = (vec![c], elapsed + 4);
        m.hire_general(c, None).unwrap();
        assert_flooded(&m, c);
    }

    #[test]
    fn a_promotion_floods_the_new_commander() {
        let mut m = model(true);
        m.world.faction_details.entry(B).or_default().bonus_base =
            vec![super::super::effects::SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new(), ..Default::default() }];
        spoil(&mut m, GB);
        m.promote_unit(ForceId(1001), 0).unwrap();
        let c = m.world.forces[&ForceId(1001)].commander.unwrap();
        assert_flooded(&m, c);
    }

    /// A coast map: sea in x 0..9, the coast at x 9..10, land from x 10 (`embark::tests::coast_map`);
    /// A's army at (12, 5) and A's navy (force 1002, admiral 103) at (6, 5); B's colonel far away.
    fn coast_model() -> (CampaignModel, CharacterId) {
        let mut m = super::super::tests::test_model();
        m.world.characters.get_mut(&GA).unwrap().position = at(12, 5);
        m.world.characters.get_mut(&GB).unwrap().position = at(18, 1);
        let c = CharacterId(103);
        m.world.characters.insert(c, Character { id: c, faction: A, kind: CharacterKind::Admiral, position: at(6, 5), movement_points: 60, max_movement_points: 60, base_movement_points: 60, garrisoned_in: None });
        let mut ship = m.world.forces[&ForceId(1000)].units[0].clone();
        ship.id = UnitId(50);
        ship.character = Some(c);
        m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(c), units: vec![ship], is_navy: true });
        let mut grid = PathGrid::new((0.0, 0.0), 2.0, 10, 6);
        grid.poly = Some(super::super::embark::tests::coast_map());
        m.attach_terrain(Terrain(Arc::new(grid)));
        (m, c)
    }

    #[test]
    fn boarding_floods_the_fleet_and_its_army() {
        let (mut m, admiral) = coast_model();
        spoil(&mut m, admiral);
        spoil(&mut m, GA);
        m.apply(CampaignCommand::Embark { force: ForceId(1000), navy: ForceId(1002) }).unwrap();
        assert_eq!(m.carrier_of(ForceId(1000)), Some(ForceId(1002)));
        assert_flooded(&m, admiral);
        assert!(!zone(&m, GA).is_empty());
    }

    #[test]
    fn a_landing_floods_the_fleet_and_the_landed_army() {
        let (mut m, admiral) = coast_model();
        for c in [GA, admiral] {
            m.world.characters.get_mut(&c).unwrap().position = at(2, 5);
        }
        m.world.embarked.insert(ForceId(1000), ForceId(1002));
        spoil(&mut m, admiral);
        spoil(&mut m, GA);
        let ev = m.apply(CampaignCommand::Disembark { force: ForceId(1000), to: at(15, 5) }).unwrap();
        assert!(ev.iter().any(|e| matches!(e, super::super::events::CampaignEvent::CharacterDisembarksNavy { .. })), "{ev:?}");
        assert_ne!(m.world.characters[&admiral].position, at(2, 5), "the fleet sailed");
        assert!(!zone(&m, admiral).is_empty());
        assert_flooded(&m, GA);
    }
}
