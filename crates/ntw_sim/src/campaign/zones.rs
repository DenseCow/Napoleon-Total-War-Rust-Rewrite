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
//! the exe keeps the flood's layer on the character (+0x1C8) until one of them. A walk floods only the
//! mover (and the armies it carries): an army an enemy walks up to keeps the zone it had. An expelled
//! army the rule places is moved and flooded at its new place; one it could not place (`to: None` in
//! [`CampaignModel::expel_from_region`], `0x008D9DC0` finding no candidate) stands where it was with its
//! old zone, until its faction is flooded again: the treaty expulsions refresh both factions inside the
//! expel branch (`0x00B13A30`, `0x00B0CA20`, `0x00B0D0C0`, `0x00B29170`, `0x00B64B10`), the cancel
//! `0x00B67BD0` ([`CampaignModel::expel_from_each_region_of`] alone) none (its command path `0x00B67B20`
//! refreshes the target afterwards, the war-abuse path `0x00B0DAF0` none).
//!
//! Not ported, none needed (`PATHFINDING.md` §8, CONFIRMED by disassembly): the script barriers'
//! refresh (`0x00B64E30`, every faction; no barriers are modelled, [`zoc::zone_in`]); `0x0096CBE0`
//! at a battle's start, which floods the characters lifted off the map in mid-move (+0x1B7, set by
//! character vtable slot +0x1C `0x00A28B50`, cleared by the move's last step at `0x00A28855`): it
//! walks the moving characters' list (the move manager at campaign +0xF60, its list +0x24, node +8
//! the character, whose movable base keeps a pointer to him at +0x20), the one `0x00973B90` steps
//! every update (`0x00972CA0` per character). A walk of ours moves whole
//! ([`CampaignModel::set_down`]), so none is ever lifted; `0x009CB4C0`, run by the force reader `0x00881120`
//! while a save is read, so it floods nothing (+0xFC7 is still set). `HireAdmiral`'s own flood
//! (`0x00A16110` at `0x00A162CA`: the new commander at the fleet's place) is the one
//! `rebuild_force_zones` makes there.
//!
//! The region gate (`PATHFINDING.md` §8, CONFIRMED by disassembly): a zone crosses into another
//! pathfinding region only when its layer's flag for that region id is set (`0x00B16CF0` →
//! `0x00B16BD0`, [`zoc::reach_in`]). The flags are set for the commander's own faction f before the
//! flood (`0x00B06280` → `0x00B78A10`): a single region is open to f when f has a record and the
//! region is no campaign region, or its owner is f or has no record, or the owner's record towards f
//! lets f stand (`0x00B622E0`, [`region_open_to`]); a group of regions is open when
//! every region of it is (an empty one always). A risen army (in a region's
//! [`World::region_risen_armies`](super::world::World::region_risen_armies) list: army +0x98, its
//! `REBEL_DATA`) gets only its home region's single slots instead (`0x00B078C0`); the load pass sets
//! the same for every rebel character (`0x00B077B0`). So a zone stops at the border of a region its
//! faction may not stand in, a rebel army's at the edge of the region it stands in and the one it rose
//! in. An army's or fleet's path search runs behind the same flags (CONFIRMED by disassembly,
//! [`CampaignModel::path_region_gate`]).
//!
//! A rebel commander's later floods, and every flood's view (`PATHFINDING.md` §8, CONFIRMED by
//! disassembly; [`GridState`]): the grid keeps the faction view its layers were last set for (+0x3FC
//! the faction, +0x414 the mode) and the single-region field +0x400. A faction view is stored by
//! `0x00B06210` with its store flag: per faction in the load pass, again by `0x00B0DF30` at the
//! campaign start, and as the acting view (`0x008ED860` → `0x008A87D0`, the turn faction's) at the
//! campaign start (`0x00873DF3`) and at each faction's turn start (`0x008F2620` at `0x008F28D3`,
//! [`CampaignModel::store_acting_view`]). A rebel view (mode 2) is stored by `0x00B077B0` with +0x400 =
//! the region index its army rose in: per rebel commander in the load pass, and before each order a
//! rebel character carries out (the campaign AI's order steps `0x00A645C0`, `0x00A65360`, ...;
//! [`CampaignModel::store_rebel_view`]). Only the grid constructor (`0x00AE7350`, 0) and `0x00B077B0`
//! write +0x400, so it keeps the last rebel commander's region. `0x00B7BAF0` (flag 0) takes the view
//! faction V from the commander during the load pass and from the stored view after it (campaign
//! +0x39, set at `0x00873D4D`). V with a record, or any V while +0x400 is 0, gets the scope
//! `0x00AF5200`: V's obstacle modes and the commander's own faction's flags; V without a record after
//! a non-zero +0x400 floods on the layers as the stored view left them, without the scope. So after
//! the load pass every flood, of any faction's commander, sees the obstacles with the turn faction's
//! (or the acting rebel's) modes. `ReapplyPathGridFactionView` (`0x00B67490`) puts a stored view back
//! on the layers after the diplomacy refreshes, which ours need not: each flood here takes its view
//! from [`GridState`].
//!
//! The acting view is a character's only in a revolution ([`CampaignModel::store_acting_view`],
//! CONFIRMED by disassembly), which ours does not model yet (PROVISIONAL there). PROVISIONAL (BACKLOG):
//! the views the post-battle steps (`0x008F7880`) and `0x008C1140` store are not modelled.
//!
//! The faction's character list (+0x768) is walked in its own order
//! ([`World::characters_in_list_order`](super::world::World::characters_in_list_order)): the loader
//! builds it in `CHARACTER_ARRAY` order (`0x0087A190`), every new character is appended
//! (`0x00878C10` → `0x008CF6B0`) and removal keeps the order (`0x008B0150`); CONFIRMED by disassembly
//! (PATHFINDING.md §8). It is not id order: the original's saves list characters out of id order. It
//! matters only for a faction's own characters, whose cores bound each other's floods in that order.
//! A flood's obstacles are taken in force order, as the path search takes them.
//!
//! A commander without a stored zone (a force made by a path that floods none, or a terrain set
//! without [`CampaignModel::attach_terrain`]) is read as his bare-map flood ([`zoc::reach`]), logged
//! once: our own fallback.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use super::expulsion::record_lets_stand;
use super::ids::{CharacterId, FactionId, ForceId};
use super::polypath::{Mover, PolyMap};
use super::world::{CampaignModel, MilitaryForce, Stance, Terrain};
use super::zoc::{self, Obstacle};
use crate::fixed::Fixed20;

type RegionOwners<'a> = std::collections::HashMap<&'a str, FactionId>;

/// Each commander's stored zone: the static polygons of the zone, sorted ([`zoc::zone_in`]).
pub type StoredZones = BTreeMap<CharacterId, Arc<[u32]>>;

/// The one empty zone every obstacle shares when there is no polygon map to flood on.
static EMPTY_ZONE: LazyLock<Arc<[u32]>> = LazyLock::new(|| Arc::from([]));

/// The model's stored zones (not saved: a load floods them again, as `0x00B0DC10` does), with the map
/// they were flooded on and the grid's stored view.
#[derive(Debug, Clone, Default)]
pub struct Zones(Option<Built>);

#[derive(Debug, Clone)]
struct Built {
    terrain: Terrain,
    zones: StoredZones,
    grid: GridState,
}

/// The faction view the path grid's layers were last left in (module docs, "A rebel commander's
/// later floods"): `CampaignPathGrid` +0x3FC (the faction) and +0x414 (the mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GridView {
    /// Mode 0, stored by `0x00B06210` with its store flag: the faction's obstacle modes and region
    /// flags on every layer.
    Faction(FactionId),
    /// Mode 2, stored by `0x00B077B0`: the rebel faction's modes and only the single slots of the
    /// region the acting rebel army rose in, which is the grid's +0x400 ([`GridState`]).
    Rebel(FactionId),
}

/// The grid's stored view with its single-region field +0x400 (the region index of the last
/// `0x00B077B0`: 0 from the grid constructor `0x00AE7350`, and never cleared after), and the
/// campaign's byte +0x39 (model +0xF5C), which makes every flood read the stored view's faction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridState {
    view: Option<GridView>,
    region: u32,
    /// Campaign +0x39: cleared by the campaign constructor (`0x0090C4A0` at `0x0090C527`), set once
    /// the load pass is done (`0x00872550` at `0x00873D4D`, before the acting view) and again at each
    /// round start (`RunCampaignRoundStart` `0x0096C054`); no store clears it after (CONFIRMED by an
    /// instruction search of the exe).
    read_view: bool,
}

impl GridState {
    /// The grid as its constructor leaves it: no view stored, +0x400 = 0.
    const NEW: GridState = GridState { view: None, region: 0, read_view: false };

    /// `0x00B077B0`: the rebel `faction`'s view of `region`, stored with +0x400.
    fn store_rebel(&mut self, faction: FactionId, region: u32) {
        self.view = Some(GridView::Rebel(faction));
        self.region = region;
    }

    /// A risen rebel commander's view stored from his home region's grid index
    /// ([`CampaignModel::risen_home_index`]'s inner value). One whose home region is not on the grid
    /// gets no stored view (our own fallback: he floods behind [`CampaignModel::region_gate`], not the
    /// previous rebel's view); the exe never reaches that case, it reads army +0x98 with no null
    /// check. The load pass and the order-time store both come through here.
    fn store_rebel_commander(&mut self, faction: FactionId, home: Option<u32>) {
        match home {
            Some(x) => self.store_rebel(faction, x),
            None => self.view = None,
        }
    }

    /// The faction of the stored view (+0x3FC).
    fn view_faction(self) -> Option<FactionId> {
        match self.view? {
            GridView::Faction(f) | GridView::Rebel(f) => Some(f),
        }
    }
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

/// `IsRegionOpenToFaction` `0x00B622E0` for a region `owner` holds (CONFIRMED by disassembly; `f`
/// has a record, the caller checked): open when `owner` is `f` or has no record (the rebels; or is
/// no loaded faction); otherwise when `owner`'s relationship record towards `f` lets it stand
/// (`0x00B64C50` on owner +0x528, then `CheckRelationshipLetsFactionStand` `0x00B16570`,
/// [`record_lets_stand`]). Not `may_stay` (`0x008D9DC0`), which also opens to the rebels and
/// through `at_war`.
///
/// ORIGINAL BUG: on a missing record `0x00B64C50` returns the owner's first record (never null) and
/// `0x00B16570` tests that one, an unrelated faction's. Ours reads the missing record as none: no
/// access, so the stance alone decides. Unreachable with shipped data: every pair of non-rebel
/// factions has a record in `eur_napoleon`'s startpos (41 factions) and in the evidence saves
/// (measured 2026-10-19).
fn region_open_to(m: &CampaignModel, owner: FactionId, f: FactionId) -> bool {
    owner == f
        || !m.world.factions.contains_key(&owner)
        || m.world.is_rebel_faction(owner)
        || record_lets_stand(m, owner, f)
}

/// `0x00B78A10`'s flag per region id of `map` from whether each region (by region index) is open:
/// a single slot by its region, a group slot only when every region of it is open (an empty group
/// always). A faction without a record (`rebel`) gets every single slot closed.
fn slot_flags(map: &PolyMap, open: &[bool], rebel: bool) -> Vec<bool> {
    let open_at = |i: &u32| open.get(*i as usize).copied().unwrap_or(!rebel);
    let flag = |(slot, set): (usize, &Vec<u32>)| if rebel && slot < map.single_region_slots { false } else { set.iter().all(open_at) };
    map.region_sets.iter().enumerate().map(flag).collect()
}

/// The flags of a layer set to one region `x` (by region index): its single slots only, every other
/// slot closed (`0x00B078C0` for an army's home region, `0x00B07870` → `0x00B07950` for a given
/// one; CONFIRMED by disassembly). `None`, a region the grid does not hold, closes every slot.
fn only_region(map: &PolyMap, x: Option<u32>) -> Vec<bool> {
    let only_x = |(slot, set): (usize, &Vec<u32>)| slot < map.single_region_slots && x.is_some_and(|x| set.as_slice() == [x]);
    map.region_sets.iter().enumerate().map(only_x).collect()
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

    /// Every force commander in faction-list order
    /// ([`World::characters_in_list_order`](super::world::World::characters_in_list_order), module
    /// docs).
    fn commanders_in_list_order(&self) -> Vec<Commander> {
        let rank: BTreeMap<CharacterId, usize> =
            self.world.characters_in_list_order().into_iter().enumerate().map(|(i, id)| (id, i)).collect();
        let mut out: Vec<Commander> = self.commanders().collect();
        out.sort_by_key(|c| rank.get(&c.id).copied().unwrap_or(usize::MAX));
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
                    (None, None) => Arc::clone(&EMPTY_ZONE),
                };
                Some(Obstacle { pos: c.pos_f32(), navy: c.navy, garrisoned: c.garrisoned, at_war, zone })
            })
            .collect()
    }

    /// Floods `c`'s zone against the obstacles the view's faction sees with their stored zones, behind
    /// the layer's region gate ([`Self::flood_view`]).
    fn flood_zone(&self, map: &PolyMap, c: &Commander, zones: &StoredZones, only_stored: bool, owners: &RegionOwners<'_>, grid: GridState) -> Arc<[u32]> {
        let (modes, gate) = self.flood_view(map, c, owners, grid);
        let others = self.obstacles_with_zones(Some(map), modes, c.id, Some(zones), only_stored);
        zoc::zone_in(map, c.pos_f32(), c.navy, c.garrisoned, &others, gate.as_deref()).into()
    }

    /// The faction whose obstacle modes `c`'s flood reads, with the region flags of its layer
    /// (`0x00B7BAF0` with flag 0; module docs, "A rebel commander's later floods"). The view faction
    /// is `c`'s own during the load pass and the stored view's (+0x3FC) after it (campaign +0x39,
    /// [`GridState`]). A view faction with a record, or any while +0x400 is 0, gets the scope
    /// (`0x00AF5200`): the view faction's modes, `c`'s own faction's flags ([`Self::region_gate`]). A
    /// view faction without a record after a non-zero +0x400 floods on the layers as the stored view
    /// left them: a faction view's modes and flags (`0x00B06210`), or the rebel view's modes with only
    /// its region's single slots (`0x00B077B0` / `0x00B07870`).
    fn flood_view(&self, map: &PolyMap, c: &Commander, owners: &RegionOwners<'_>, grid: GridState) -> (FactionId, Option<Vec<bool>>) {
        let view = if grid.read_view { grid.view_faction().unwrap_or(c.faction) } else { c.faction };
        if self.world.is_rebel_faction(view) && grid.region != 0 {
            match grid.view {
                Some(GridView::Faction(f)) => {
                    // `map` is the terrain's polygon map, so the terrain is there.
                    if let Some(t) = &self.terrain {
                        let open = self.regions_open_to(&t.0.region_keys, f, owners);
                        return (f, Some(slot_flags(map, &open, self.world.is_rebel_faction(f))));
                    }
                }
                Some(GridView::Rebel(f)) => return (f, Some(only_region(map, Some(grid.region)))),
                // +0x400 is written only with a rebel view (`0x00B077B0`), so the exe always has a view
                // stored here; ours has none only after its load-pass fallback ([`Self::build_zones`]).
                None => {}
            }
        }
        (view, self.region_gate(map, c, owners))
    }

    /// The region index of the region `force` rose in (army +0x98, its `REBEL_DATA`: the region list
    /// [`World::region_risen_armies`](super::world::World::region_risen_armies) holds it in), if it
    /// is a risen army; `Some(None)` for one whose region the grid does not hold.
    fn risen_home_index(&self, force: ForceId) -> Option<Option<u32>> {
        let home = self.world.region_risen_armies.iter().find(|(_, armies)| armies.contains(&force)).map(|(r, _)| *r)?;
        let grid = self.terrain.as_ref().map(|t| &t.0);
        Some(self.world.regions.get(&home).and_then(|r| grid?.region_index(&r.key)))
    }

    /// The owner of each region by key: built once per flood pass, not per flood.
    fn region_owners(&self) -> RegionOwners<'_> {
        self.world.regions.values().map(|r| (r.key.as_str(), r.owner)).collect()
    }

    /// The region flags of `c`'s layer for his flood, one per region id of `map` (module docs, "The
    /// region gate"); `None` without a movement grid to look the regions up in.
    fn region_gate(&self, map: &PolyMap, c: &Commander, owners: &RegionOwners<'_>) -> Option<Vec<bool>> {
        let grid = &self.terrain.as_ref()?.0;
        // A risen army (army +0x98 set): only the single slots of its home region X (`0x00B078C0`).
        if let Some(x) = self.risen_home_index(c.force) {
            return Some(only_region(map, x));
        }
        // A rebel commander with no risen-army entry keeps the faction flags (every single region closed, empty groups open): our own fallback; the exe never reaches it (its load pass 0x00B0DC10 calls 0x00B077B0 then 0x00B078C0, which reads army +0x98 with no null check).
        let rebel = self.world.is_rebel_faction(c.faction);
        let open = self.regions_open_to(&grid.region_keys, c.faction, owners);
        Some(slot_flags(map, &open, rebel))
    }

    /// The single-region half of `0x00B78A10` for faction `f`: whether each region of the grid (by
    /// region index, `keys` its keys) is open to `f` through `0x00B622E0` ([`region_open_to`]);
    /// every one closed to a faction without a record, every region nobody owns open to the others.
    fn regions_open_to(&self, keys: &[String], f: FactionId, owners: &RegionOwners<'_>) -> Vec<bool> {
        if self.world.is_rebel_faction(f) {
            return vec![false; keys.len()];
        }
        keys.iter().map(|key| owners.get(key.as_str()).is_none_or(|&owner| region_open_to(self, owner, f))).collect()
    }

    /// The region flags a path search for a mover of `faction` runs behind, one per region id of the
    /// polygon map (`PATHFINDING.md` §8, "The region gate"; CONFIRMED by disassembly): the flags of
    /// the faction view the grid's layers hold (`0x00B06210` → `0x00B06280` → `0x00B78A10`, the
    /// acting faction's), the zone flood's rule. Every search's neighbour test applies them
    /// (`0x00B16CF0` / `0x00B16BA0` in `0x00B2CC20` and the other searches,
    /// [`super::polypath::View::gate_lets`]); the caller leaves them off for an agent, whose searches
    /// clear the gate byte +0x168 (the scopes `0x00AF5B60` / `0x00AF5C00` with their first argument
    /// set unless the mover leads an army, `0x009D0F50`, or a fleet, `0x009D0F80`).
    ///
    /// `target`: a search towards a character or settlement standing there (the queries `0x00B20600`
    /// … `0x00B25470`, e.g. `0x00B21510`). Its scope `0x00AF5C00` opens the regions of the owner O of
    /// the campaign region at that point (`0x00AA1030`; ours [`PathGrid::region_at`]) on top for the
    /// search and closes them after (`0x00AF7BE0`), through `0x00B628E0`: for each region of O's list
    /// (faction +0x778) whose settlement lies inside the grid (`0x00B1FC40`), the slot of the polygon
    /// located at the settlement (`0x00B53930`, mover type 6; ours [`PolyMap::locate`]) is set
    /// (`0x00B620A0`), then every group slot all of whose regions are now open. The exe does this
    /// only when O is not the mover's faction, both have a record and O's record towards the mover
    /// does not let it stand; otherwise O's regions are open already, which ours relies on.
    ///
    /// `None` without a polygon map, and for the rebels: PROVISIONAL (BACKLOG): a recordless (rebel)
    /// mover's search runs on the layers as the grid's stored view left them, which ours keeps in
    /// [`GridState`] (after his own order `0x00B077B0`'s view: only his home region's single slots,
    /// +0x400). Missing: this gate does not read [`GridState`] yet, so a rebel force paths with no
    /// gate at all, and whether the search scopes (`0x00AF5B60` / `0x00AF5C00`, and `0x00B67490`
    /// mode 2 behind them) keep, re-store or widen that view for a recordless mover is not traced.
    ///
    /// [`PathGrid::region_at`]: super::pathing::PathGrid::region_at
    pub(crate) fn path_region_gate(&self, faction: FactionId, target: Option<(f32, f32)>) -> Option<Vec<bool>> {
        let grid = &self.terrain.as_ref()?.0;
        let map = grid.poly.as_ref()?;
        if self.world.is_rebel_faction(faction) {
            return None;
        }
        let owners = self.region_owners();
        let mut open = self.regions_open_to(&grid.region_keys, faction, &owners);
        let mut groups_opened = Vec::new();
        if let Some(o) = target.and_then(|(x, z)| grid.region_at(x, z)).and_then(|k| owners.get(k).copied()) {
            for r in self.world.regions.values().filter(|r| r.owner == o) {
                let (x, z) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
                // PROVISIONAL: the settlement is located with Mover::Land, mode 2; the exe uses
                // `0x00B53930` type 6, not shown equal.
                let Some(p) = map.cell_of(x, z).and_then(|_| map.locate(x, z, Mover::Land, 2)) else { continue };
                let id = map.region_id[p] as usize;
                match map.region_sets.get(id) {
                    Some(set) if id < map.single_region_slots => {
                        for &i in set {
                            if let Some(flag) = open.get_mut(i as usize) {
                                *flag = true;
                            }
                        }
                    }
                    Some(_) => groups_opened.push(id),
                    None => {}
                }
            }
        }
        let mut flags = slot_flags(map, &open, false);
        for id in groups_opened {
            flags[id] = true;
        }
        Some(flags)
    }

    /// The load pass `0x00B0DC10` (module docs): every commander flooded in turn order, the rebels
    /// last, each against those flooded before it; with the grid's view as the campaign start leaves
    /// it (module docs, "A rebel commander's later floods").
    fn build_zones(&self, map: &PolyMap) -> (StoredZones, GridState) {
        let commanders = self.commanders_in_list_order();
        let mut zones = BTreeMap::new();
        let owners = self.region_owners();
        let mut grid = GridState::NEW;
        for f in self.world.factions_in_turn_order() {
            // A faction's view is stored before its characters (`0x00B06210(f, 1)`); each rebel
            // character's own before his flood (`0x00B077B0`).
            let rebel = self.world.is_rebel_faction(f);
            if !rebel {
                grid.view = Some(GridView::Faction(f));
            }
            for c in commanders.iter().filter(|c| c.faction == f) {
                if rebel {
                    // No risen-army entry at all floods behind the faction flags ([`Self::region_gate`]).
                    grid.store_rebel_commander(f, self.risen_home_index(c.force).flatten());
                }
                let z = self.flood_zone(map, c, &zones, true, &owners, grid);
                zones.insert(c.id, z);
            }
        }
        // The campaign start (`0x00872550`) then runs `0x00B0DF30`, which stores the same views again
        // (the rebels' last, so +0x400 keeps the last rebel commander's region), and stores the acting
        // view (`0x008ED860` at `0x00873DF3`).
        if let Some(view) = self.acting_view() {
            grid.view = Some(view);
        }
        // Then campaign +0x39 is set (`0x00873D4D`): every later flood reads the stored view.
        grid.read_view = true;
        (zones, grid)
    }

    /// The acting view `0x008ED860` stores (`0x008A87D0`, CONFIRMED by disassembly): the acting
    /// faction's (`0x00974EF0`), which is the turn faction (campaign +0x18,
    /// [`super::turn::TurnState::current`]) unless that faction is in a revolution (+0x814 set,
    /// `0x008CA0B0`) with +0x81C == +0x81D, when it is campaign +0x1C. Only when the acting faction is
    /// not the turn faction (`0x00898B80`) and has no record (`0x008CEEF0`) does `0x008A87D0` look at
    /// the turn faction's revolution settlement (+0x818, set by `0x008EE3E0`; `0x008B7810`): the first
    /// entry of its +0x220 list (count +0x22C via `0x006A3110`, items +0x230) whose +0x74 handle
    /// resolves (`0x00898E80`) gives a character, whose rebel view (`0x00B077B0`) is stored instead.
    /// So in the rebels' own turn the stored view is the rebels' faction view, as ours. `None` before
    /// any turn.
    ///
    /// PROVISIONAL (BACKLOG §6 revolutions, faction +0x814 / +0x81C): revolutions are not in the
    /// model, so ours always takes the turn faction and never the revolution branch.
    fn acting_view(&self) -> Option<GridView> {
        self.turn.current.map(GridView::Faction)
    }

    /// `0x008ED860`: the acting view stored, at a faction's turn start (`0x008F2620` at `0x008F28D3`).
    pub(crate) fn store_acting_view(&mut self) {
        let view = self.acting_view();
        if let (Some(b), Some(view)) = (self.zones.0.as_mut(), view) {
            b.grid.view = Some(view);
        }
    }

    /// `0x00B077B0` for a rebel character about to carry out an order: the campaign AI's order
    /// steps store the acting character's view first when his faction has no record (`0x00A645C0`,
    /// `0x00A65360` and the other callers in `0x00A6xxxx`–`0x00AAxxxx`; CONFIRMED by decompile).
    /// `force` is the order's force, whose commander is the acting character. Nothing for a force of a
    /// faction with a record, one without a commander, or one that is no risen army (our own
    /// fallback: the exe reads army +0x98 with no null check). A risen army whose home region is not on
    /// the grid has no region to store: it clears the view, as in the load pass
    /// ([`GridState::store_rebel_commander`]). Nothing is stored before the zones are built.
    pub(crate) fn store_rebel_view(&mut self, force: ForceId) {
        let Some(f) = self.world.forces.get(&force).filter(|f| self.world.is_rebel_faction(f.faction)) else { return };
        let Some(c) = self.commander_of_force(f) else { return };
        let Some(home) = self.risen_home_index(c.force) else { return };
        if let Some(b) = self.zones.0.as_mut() {
            b.grid.store_rebel_commander(f.faction, home);
        }
    }

    /// The grid's stored view.
    pub(crate) fn grid_state(&self) -> Option<GridState> {
        self.zones.0.as_ref().map(|b| b.grid)
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
            (Some(t), Some(map)) => {
                let (zones, grid) = self.build_zones(map);
                Some(Built { terrain: t.clone(), zones, grid })
            }
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
    /// obstacles the grid's stored view faction ([`Self::flood_view`]) sees, and stores it; a
    /// character who commands no force loses his entry. Nothing without zones flooded for the current
    /// map ([`Self::attach_terrain`]).
    pub(crate) fn rebuild_zone(&mut self, character: CharacterId) {
        let (Some(map), Some(zones), Some(grid)) = (self.poly_map(), self.stored_zones(), self.grid_state()) else { return };
        let new = self.commander(character).map(|c| self.flood_zone(map, &c, zones, false, &self.region_owners(), grid));
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
    /// navy carrying this army) rebuilt after the force changed: the zone steps alone
    /// ([`Self::rebuild_force_zones_and_sight`] adds its sight steps).
    pub(crate) fn rebuild_force_zones(&mut self, force: ForceId) {
        for c in self.force_zone_characters(force) {
            self.rebuild_zone(c);
        }
    }

    /// `0x008F9400` with its sight steps: `0x00A285C0` on each of [`Self::rebuild_force_zones`]'
    /// characters in turn floods his zone (`0x00A28440(0)`), runs the core `0x009CC0E0`, then adds
    /// his sight (`0x00A27F10`); the hidden flags are left as they are (CONFIRMED by disassembly).
    /// The model adds sight as the faction's whole visible set ([`Self::refresh_shroud`]), which zone
    /// floods do not change, so each faction is refreshed once, after its first character's zone:
    /// its later sight steps would add nothing.
    pub(crate) fn rebuild_force_zones_and_sight(&mut self, force: ForceId) {
        let mut refreshed: Vec<FactionId> = Vec::new();
        for c in self.force_zone_characters(force) {
            self.rebuild_zone(c);
            let Some(faction) = self.world.characters.get(&c).map(|x| x.faction) else { continue };
            if !refreshed.contains(&faction) {
                refreshed.push(faction);
                self.refresh_shroud(faction, false);
            }
        }
    }

    /// The characters `0x008F9400` rebuilds, in its order: the force's commander, then the commanders
    /// of the armies a navy carries, or of the navy carrying this army. (The exe also runs every unit
    /// character of a linked force and the force's +0x6C list; the model keeps zones for commanders
    /// only, and its sight is the faction's whole set.) Empty without a commander.
    fn force_zone_characters(&self, force: ForceId) -> Vec<CharacterId> {
        let Some(c) = self.world.forces.get(&force).and_then(|f| f.commander) else { return Vec::new() };
        let mut linked: Vec<ForceId> = self.passengers_of(force);
        linked.extend(self.carrier_of(force));
        let commanders = linked.into_iter().filter_map(|f| self.world.forces.get(&f).and_then(|f| f.commander));
        std::iter::once(c).chain(commanders).collect()
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
        let commanders = self.commanders_in_list_order();
        if let Some(zones) = self.stored_zones_mut() {
            let ids: std::collections::BTreeSet<CharacterId> = commanders.iter().map(|c| c.id).collect();
            zones.retain(|id, _| ids.contains(id));
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
    /// when `war`; not yet on a map. It is A's turn, so every flood after the load pass reads A's
    /// stored view (module docs, "A rebel commander's later floods").
    fn world(war: bool) -> CampaignModel {
        let mut m = super::super::tests::test_model();
        m.turn.current = Some(A);
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

    /// `c`'s zone flooded now against the stored zones of what the grid's stored view sees, behind
    /// that view's gate ([`CampaignModel::flood_view`]).
    fn fresh(m: &CampaignModel, c: CharacterId) -> Vec<u32> {
        let k = m.commander(c).expect("a commander");
        let (modes, gate) = m.flood_view(map(m), &k, &m.region_owners(), m.grid_state().expect("zones flooded"));
        zone_in(map(m), k.pos_f32(), k.navy, k.garrisoned, &m.obstacles_with_stored_zones(modes, c), gate.as_deref())
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
        assert_eq!(a, zone_in(map(&m), (41.0, 21.0), false, false, &[obstacle((49.0, 21.0), zone(&m, GB))], None));
        assert!(a.len() < bare(&m, (41.0, 21.0)).len());
        // Not saved: a loaded copy floods the same zones again when its map is attached.
        let mut loaded = m.clone();
        loaded.zones = Zones::default();
        assert!(loaded == m);
        assert_eq!(loaded.stored_zones(), None, "a read builds nothing");
        loaded.build_stored_zones();
        assert_eq!(loaded.stored_zones(), m.stored_zones());
    }

    /// The load pass walks a faction's commanders in its list's order (`World::character_order`,
    /// +0x768), not in id order: the one listed first floods before the other's core is placed.
    #[test]
    fn the_load_pass_floods_a_factions_commanders_in_list_order() {
        let run = |order: Vec<CharacterId>| {
            let mut m = world(false);
            let c = second_army(&mut m);
            m.world.characters.get_mut(&c).unwrap().position = at(44, 21);
            m.world.character_order = order;
            m.attach_terrain(open_terrain());
            (m, c)
        };
        // No order listed: id order, GA (100) before the second army (103).
        let (by_id, c) = run(Vec::new());
        assert_eq!(zone(&by_id, c), fresh(&by_id, c), "flooded last");
        // Listed the other way round: the second army floods first and GA's flood stops at its core.
        let (listed, _) = run(vec![c, GB, GA]);
        assert_eq!(zone(&listed, GA), fresh(&listed, GA), "flooded last");
        assert_ne!(zone(&listed, GA), zone(&by_id, GA));
        assert_ne!(zone(&listed, c), zone(&by_id, c));
    }

    /// A removed character leaves the faction list (`World::character_order`) too, the rest keep order.
    #[test]
    fn a_removed_character_leaves_the_list_order() {
        let mut m = world(false);
        let c = second_army(&mut m);
        m.world.character_order = vec![c, GB, GA];
        assert!(m.world.remove_character(GB).is_some());
        assert_eq!(m.world.character_order, vec![c, GA]);
        assert!(m.world.remove_character(GB).is_none());
    }

    /// A faction transfer (`0x00A1B4E0`) moves the character to the end of the list, behind every
    /// character made before it (listed or not), the rest keeping their order.
    #[test]
    fn a_transferred_character_goes_to_the_end_of_the_list() {
        let mut m = world(false);
        let c = second_army(&mut m);
        m.world.character_order = vec![GB, GA];
        let before = m.world.characters_in_list_order();
        assert_eq!(before.last(), Some(&c), "unlisted, after the listed ones");
        m.world.transfer_character(GB, REBELS);
        assert_eq!(m.world.characters[&GB].faction, REBELS);
        let mut expected: Vec<CharacterId> = before.into_iter().filter(|x| *x != GB).collect();
        expected.push(GB);
        assert_eq!(m.world.characters_in_list_order(), expected);
        assert_eq!(m.world.character_order, expected, "the whole order listed");
        // An unknown character changes nothing.
        m.world.transfer_character(CharacterId(9999), REBELS);
        assert_eq!(m.world.character_order, expected);
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
        let a_cut = zone_in(map(&m), (41.0, 21.0), false, false, &[obstacle((49.0, 21.0), bare(&m, (49.0, 21.0)))], None);
        let b_cut = zone_in(map(&m), (49.0, 21.0), false, false, &[obstacle((41.0, 21.0), bare(&m, (41.0, 21.0)))], None);
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
    fn a_walk_floods_the_movers_zone() {
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
        // Moved with no event after it: the stored zone keeps the old place.
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
        // B first, against A's peace-time zone, with A's modes (A's turn, the stored view): A's own
        // general is no enemy to the view, so only his core cuts; then A against B's new one.
        let friend = Obstacle { at_war: false, ..obstacle((41.0, 21.0), before_a) };
        assert_eq!(zone(&m, GB), zone_in(map(&m), (49.0, 21.0), false, false, &[friend], None));
        assert_eq!(zone(&m, GA), fresh(&m, GA));
        assert!(holds(&m, &zone(&m, GB), mid));
        assert!(!holds(&m, &zone(&m, GA), mid), "A's zone, flooded last, stops at B's");
        m.make_peace_rules(A, B, false).unwrap();
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
        m.cancel_military_access(A, B, &mut Vec::new());
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
        m.diplomacy_round_end(A, &mut Vec::new());
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

    /// A second army of A's, commanded by character 103 at (40, 25), with one unit; both armies'
    /// commanders ride their own units, as in every save (the transfers' commander pick reads them).
    fn second_army(m: &mut CampaignModel) -> CharacterId {
        let c = CharacterId(103);
        let mut ch = m.world.characters[&GA].clone();
        ch.id = c;
        ch.position = at(40, 25);
        m.world.characters.insert(c, ch);
        let mut unit = m.world.forces[&ForceId(1000)].units[0].clone();
        unit.id = UnitId(77);
        unit.character = Some(c);
        m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].character = Some(GA);
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
        // The target has room for one unit only: the source keeps the other and its commander.
        let mut m = full_target_and_two_unit_source();
        let c = CharacterId(103);
        spoil(&mut m, GA);
        spoil(&mut m, c);
        m.merge_units(ForceId(1002), ForceId(1000), &mut Vec::new());
        assert_eq!(m.world.forces[&ForceId(1002)].units.len(), 1);
        assert_flooded(&m, GA);
        assert_flooded(&m, c);

        // Into a garrison (`0x008D5F10`): `0x008F9400` runs on the source alone (`0x008D71C1`).
        let mut m = full_target_and_two_unit_source();
        m.world.characters.get_mut(&GA).unwrap().garrisoned_in = Some(RegionId(10));
        let c = CharacterId(103);
        spoil(&mut m, GA);
        spoil(&mut m, c);
        m.merge_units(ForceId(1002), ForceId(1000), &mut Vec::new());
        assert_eq!(m.world.forces[&ForceId(1002)].units.len(), 1);
        assert!(zone(&m, GA).is_empty(), "the garrison is not flooded again");
        assert_flooded(&m, c);
    }

    /// [`second_army`] with a second, characterless unit, on the map, and A's first army one unit
    /// short of the cap.
    fn full_target_and_two_unit_source() -> CampaignModel {
        let mut m = world(true);
        let c = second_army(&mut m);
        let mut extra = m.world.forces[&ForceId(1002)].units[0].clone();
        extra.id = UnitId(78);
        extra.character = None;
        m.world.forces.get_mut(&ForceId(1002)).unwrap().units.push(extra);
        m.attach_terrain(open_terrain());
        let max = m.max_units(false);
        let filler = m.world.forces[&ForceId(1000)].units[0].clone();
        let units = &mut m.world.forces.get_mut(&ForceId(1000)).unwrap().units;
        while units.len() < max - 1 {
            let mut u = filler.clone();
            u.id = UnitId(200 + units.len() as i32);
            u.character = None;
            units.push(u);
        }
        assert!(m.stored_zones().unwrap().contains_key(&c));
        m
    }

    /// A source that empties into a garrison (`0x008D5F10`, its empty flag `0x008D65BE` tested at
    /// `0x008D6F43`) floods neither force; its commander, now in no force, loses his entry.
    #[test]
    fn a_merge_into_a_garrison_floods_neither_force() {
        let mut m = world(true);
        let c = second_army(&mut m);
        m.world.characters.get_mut(&GA).unwrap().garrisoned_in = Some(RegionId(10));
        m.attach_terrain(open_terrain());
        spoil(&mut m, GA);
        m.merge_units(ForceId(1002), ForceId(1000), &mut Vec::new());
        assert!(!m.world.forces.contains_key(&ForceId(1002)));
        assert!(zone(&m, GA).is_empty(), "the garrison is not flooded again");
        assert!(!m.stored_zones().unwrap().contains_key(&c));
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
        m.turn.current = Some(A);
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

    /// Every unit can hide on the campaign map, and `cs` have details to hold the flag.
    fn all_stealthy(m: &mut CampaignModel, cs: &[CharacterId]) {
        let mut rules = (*m.rules).clone();
        for r in rules.units.values_mut() {
            r.campaign_stealth = true;
        }
        m.rules = Arc::new(rules);
        for c in cs {
            m.world.character_details.entry(*c).or_default();
        }
    }

    /// A walk's last step sets the walker down (`0x00A285E0` with 1, `0x00972EB1`): his hidden flag is
    /// recomputed, and only his (`0x009D3000` at `0x00A2885C` runs on `this`): a navy's walk leaves
    /// the flag of the army aboard as it was.
    #[test]
    fn a_walk_sets_the_walkers_hidden_flag_only() {
        let (mut m, admiral) = coast_model();
        all_stealthy(&mut m, &[GA, admiral]);
        m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: at(15, 5) }).unwrap();
        assert!(m.world.character_details[&GA].hidden);

        let (mut m, admiral) = coast_model();
        all_stealthy(&mut m, &[GA, admiral]);
        m.world.characters.get_mut(&GA).unwrap().position = at(6, 5);
        m.world.embarked.insert(ForceId(1000), ForceId(1002));
        m.world.character_details.get_mut(&GA).unwrap().hidden = false;
        m.apply(CampaignCommand::MoveForce { force: ForceId(1002), to: at(2, 5) }).unwrap();
        assert_ne!(m.world.characters[&admiral].position, at(6, 5), "the fleet sailed");
        assert!(m.world.character_details[&admiral].hidden);
        assert!(!m.world.character_details[&GA].hidden, "the carried army's flag is not re-evaluated");
    }

    /// A landing's fleet sail ends like a walk: the admiral's hidden flag is recomputed.
    #[test]
    fn a_landing_sets_the_admirals_hidden_flag() {
        let (mut m, admiral) = coast_model();
        for c in [GA, admiral] {
            m.world.characters.get_mut(&c).unwrap().position = at(2, 5);
        }
        m.world.embarked.insert(ForceId(1000), ForceId(1002));
        all_stealthy(&mut m, &[GA, admiral]);
        m.apply(CampaignCommand::Disembark { force: ForceId(1000), to: at(15, 5) }).unwrap();
        assert_ne!(m.world.characters[&admiral].position, at(2, 5), "the fleet sailed");
        assert!(m.world.character_details[&admiral].hidden);
    }

    /// The landed army is set down at the landing (`0x00A285E0` with 1 from the step mover at the
    /// route's end, `0x00972EB1` / `0x00973A01`, after the `navy_disembark` and `teleport` waypoints
    /// `0x0094BD50` / `0x0090B720` of `0x00918750`): its sight from the landing is added to the
    /// shroud and its own hidden flag is recomputed.
    #[test]
    fn a_landing_sets_the_landed_army_down() {
        use super::super::visibility::{CellSet, Shroud, SightGrid};
        let (mut m, admiral) = coast_model();
        for c in [GA, admiral] {
            m.world.characters.get_mut(&c).unwrap().position = at(2, 5);
        }
        m.world.embarked.insert(ForceId(1000), ForceId(1002));
        all_stealthy(&mut m, &[GA, admiral]);
        // Only the army sees, 1 unit around it; nothing is visible yet.
        let ids: Vec<CharacterId> = m.world.characters.keys().copied().collect();
        for c in ids {
            m.world.sight_radius.insert(c, if c == GA { 1.0 } else { 0.0 });
        }
        m.world.regions.clear();
        m.world.sight_grid = Some(SightGrid::centred(64, 64, 64));
        m.world.shrouds.insert(A, Shroud { explored: CellSet::new(64, 64), visible: CellSet::new(64, 64), hidden: CellSet::new(64, 64), active: true });
        m.apply(CampaignCommand::Disembark { force: ForceId(1000), to: at(15, 5) }).unwrap();
        assert_eq!(m.carrier_of(ForceId(1000)), None, "landed");
        let (x, z) = m.world.characters[&GA].position;
        let east = (x.to_f32() + 0.9, z.to_f32());
        assert!(m.sees(A, east), "the army's sight from the landing");
        assert!(m.world.character_details[&GA].hidden, "the landed army's hidden flag");
        assert_flooded(&m, GA);
    }

    /// The rebels of the region-gate tests (a faction without a key).
    const REBELS: FactionId = FactionId(9);

    /// An 80 x 40 open map of four pathfinding regions by column: x < 40 is region id 0 (the single
    /// region `test_region_10`, A's), x 40..60 id 1 (`test_region_11`, B's), x 60..70 id 2 (a group of
    /// both) and x 70..80 id 3 (an empty group). The campaign regions' cells match the single ids.
    fn region_terrain() -> Terrain {
        let mut grid = PathGrid::new((0.0, 0.0), 2.0, 40, 20);
        grid.region_keys = vec!["test_region_10".into(), "test_region_11".into()];
        let id = |c: u32| match c {
            0..20 => 0,
            20..30 => 1,
            30..35 => 2,
            _ => 3,
        };
        for (i, r) in grid.region.iter_mut().enumerate() {
            let c = id(i as u32 % 40);
            *r = if c < 2 { c } else { u32::MAX };
        }
        grid.poly = Some(super::super::zoc::tests::open_regions(40, 20, id, vec![vec![0], vec![1], vec![0, 1], vec![]], 2));
        Terrain(Arc::new(grid))
    }

    /// [`world`] (at peace) with A's general at (38, 21), 2 units from the border of B's region 11,
    /// B's colonel out of the way at (75, 5), and the rebels; not yet on a map.
    fn border_world() -> CampaignModel {
        let mut m = world(false);
        m.world.characters.get_mut(&GA).unwrap().position = at(38, 21);
        m.world.characters.get_mut(&GB).unwrap().position = at(75, 5);
        let mut rebels = m.world.factions[&B].clone();
        rebels.id = REBELS;
        rebels.key = String::new();
        m.world.factions.insert(REBELS, rebels);
        m
    }

    /// `c`'s region flags, one per region id.
    fn gate(m: &CampaignModel, c: CharacterId) -> Vec<bool> {
        m.region_gate(map(m), &m.commander(c).expect("a commander"), &m.region_owners()).expect("a movement grid")
    }

    /// The easternmost polygon centre of `c`'s stored zone.
    fn east(m: &CampaignModel, c: CharacterId) -> f32 {
        zone(m, c).iter().map(|&p| map(m).centre(map(m).poly_cell[p as usize]).0).fold(0.0, f32::max)
    }

    /// A on a region map where B holds a relationship record towards A edited by `edit`.
    fn with_record(edit: impl FnOnce(&mut CampaignModel)) -> CampaignModel {
        let mut m = border_world();
        m.world.relationships.insert((B, A), super::super::details::Relationship::default());
        edit(&mut m);
        m.attach_terrain(region_terrain());
        m
    }

    #[test]
    fn a_zone_stops_at_a_region_whose_record_does_not_let_its_faction_stand() {
        let m = with_record(|_| {});
        // A's own region open, B's closed, the group of both closed, the empty group open.
        assert_eq!(gate(&m, GA), [true, false, false, true]);
        assert!(east(&m, GA) < 40.0, "{}", east(&m, GA));
        assert_eq!(zone(&m, GA), fresh(&m, GA));
        // An alliance alone does not open it (not `may_stay`'s rule).
        let m = with_record(|m| m.world.set_stance(B, A, Stance::Allied).unwrap());
        assert_eq!(gate(&m, GA), [true, false, false, true]);
    }

    #[test]
    fn a_missing_record_reads_as_no_access_and_a_recordless_owner_opens_its_region() {
        // No (B, A) record: no access, so only the stance can open B's region (the exe would test
        // B's first, unrelated record: ORIGINAL BUG of 0x00B64C50, see `region_open_to`).
        let mut m = border_world();
        m.attach_terrain(region_terrain());
        assert!(!m.world.relationships.contains_key(&(B, A)));
        assert_eq!(gate(&m, GA), [true, false, false, true]);
        assert!(east(&m, GA) < 40.0, "{}", east(&m, GA));
        let mut m = border_world();
        m.world.set_stance(B, A, Stance::War).unwrap();
        m.attach_terrain(region_terrain());
        assert!(!m.world.relationships.contains_key(&(B, A)));
        assert_eq!(gate(&m, GA), [true; 4]);
        assert!(east(&m, GA) > 40.0, "{}", east(&m, GA));
        // A rebel owner, or one that is no loaded faction, opens its region even with a record.
        for owner in [REBELS, FactionId(99)] {
            let m = with_record(|m| {
                m.world.relationships.insert((owner, A), super::super::details::Relationship::default());
                m.world.regions.get_mut(&RegionId(11)).unwrap().owner = owner;
            });
            assert_eq!(gate(&m, GA), [true; 4], "{owner:?}");
        }
    }

    #[test]
    fn a_record_that_lets_the_faction_stand_opens_the_owners_region() {
        let edits: [fn(&mut CampaignModel); 4] = [
            |m| m.world.relationships.get_mut(&(B, A)).unwrap().military_access_turns = 1,
            |m| m.world.relationships.get_mut(&(B, A)).unwrap().allied_in_war_against.push(super::super::details::AlliedWar { enemy: FactionId(3), saved_access_turns: 0 }),
            |m| m.world.set_stance(B, A, Stance::Protectorate).unwrap(),
            |m| m.world.set_stance(B, A, Stance::War).unwrap(),
        ];
        for (k, edit) in edits.into_iter().enumerate() {
            let m = with_record(edit);
            assert_eq!(gate(&m, GA), [true; 4], "case {k}");
            assert!(east(&m, GA) > 40.0, "case {k}");
        }
    }

    #[test]
    fn a_rebel_commander_keeps_to_the_region_it_stands_in() {
        let mut m = border_world();
        m.world.forces.get_mut(&ForceId(1000)).unwrap().faction = REBELS;
        m.world.characters.get_mut(&GA).unwrap().faction = REBELS;
        m.attach_terrain(region_terrain());
        // Every single region closed (its own too, which its flood needs no flag for), groups by
        // their regions: only the empty group stays open.
        assert_eq!(gate(&m, GA), [false, false, false, true]);
        assert!(!zone(&m, GA).is_empty() && east(&m, GA) < 40.0, "{}", east(&m, GA));
    }

    #[test]
    fn a_risen_army_keeps_to_its_home_region_until_it_leaves_the_world() {
        let mut m = border_world();
        m.world.region_risen_armies.insert(RegionId(11), vec![ForceId(1000)]);
        m.attach_terrain(region_terrain());
        // Only its home region's single slot (`0x00B078C0`), whatever the owners' records say.
        assert_eq!(gate(&m, GA), [false, true, false, false]);
        assert!(east(&m, GA) > 40.0, "{}", east(&m, GA));
        m.remove_force(ForceId(1000), None);
        assert!(m.world.region_risen_armies.is_empty());
    }

    /// Whether `c` finds a path to `to`, as a move ([`CampaignModel::plan_path`]) and as a search
    /// towards a character or settlement there ([`CampaignModel::plan_path_to_target`]).
    fn paths(m: &CampaignModel, c: CharacterId, to: (Fixed20, Fixed20)) -> (bool, bool) {
        (m.plan_path(c, to).is_some(), m.plan_path_to_target(c, to).is_some())
    }

    #[test]
    fn an_army_paths_only_into_regions_its_faction_may_stand_in() {
        // Into B's region 11 (x 40..60) and on to the group of both (x 60..70): closed to A without
        // access; A's own region needs no flag. A target there opens B's regions for the search,
        // but only the slot its settlement stands in.
        let mut m = with_record(|_| {});
        let (inside, beyond, home) = (at(50, 21), at(65, 21), at(20, 21));
        assert_eq!(paths(&m, GA, inside), (false, false), "B's settlement stands in A's region");
        assert_eq!(paths(&m, GA, home), (true, true));
        m.world.regions.get_mut(&RegionId(11)).unwrap().settlement.position = at(52, 31);
        assert_eq!(paths(&m, GA, inside), (false, true));
        // The group slot opens once all its regions are open: a target in it opens B's region 11
        // (its own region at that point is no campaign region, so nothing more).
        assert_eq!(paths(&m, GA, beyond), (false, false));
        // Access opens B's regions to every search.
        m.world.relationships.get_mut(&(B, A)).unwrap().military_access_turns = 1;
        assert_eq!(paths(&m, GA, inside), (true, true));
        assert_eq!(paths(&m, GA, beyond), (true, true));
    }

    #[test]
    fn an_agent_and_a_rebel_force_path_without_the_gate() {
        // An agent's searches clear the gate byte (0x00AF5B60 / 0x00AF5C00 with the flag set).
        let mut m = with_record(|_| {});
        m.world.forces.remove(&ForceId(1000));
        assert_eq!(paths(&m, GA, at(50, 21)), (true, true));
        // A rebel force: no gate (PROVISIONAL, `path_region_gate`).
        let mut m = border_world();
        m.world.forces.get_mut(&ForceId(1000)).unwrap().faction = REBELS;
        m.world.characters.get_mut(&GA).unwrap().faction = REBELS;
        m.attach_terrain(region_terrain());
        assert_eq!(paths(&m, GA, at(50, 21)), (true, true));
    }

    #[test]
    fn the_path_gate_has_the_zone_floods_flags() {
        for edit in [(|_: &mut CampaignModel| {}) as fn(&mut CampaignModel), |m| m.world.set_stance(B, A, Stance::War).unwrap()] {
            let m = with_record(edit);
            assert_eq!(m.path_region_gate(A, None), Some(gate(&m, GA)));
        }
    }

    /// [`border_world`] with A's general a rebel army risen in `home`, loaded onto the region map.
    fn risen_rebel(home: RegionId) -> CampaignModel {
        let mut m = border_world();
        m.world.forces.get_mut(&ForceId(1000)).unwrap().faction = REBELS;
        m.world.characters.get_mut(&GA).unwrap().faction = REBELS;
        m.world.region_risen_armies.insert(home, vec![ForceId(1000)]);
        m.attach_terrain(region_terrain());
        m
    }

    /// `f`'s turn start stores its view (`0x008ED860`), then the rebel's zone is flooded again.
    fn reflood_in_turn_of(m: &mut CampaignModel, f: FactionId) {
        m.turn.current = Some(f);
        m.store_acting_view();
        m.rebuild_zone(GA);
    }

    #[test]
    fn a_rebel_commanders_later_flood_reads_the_grids_stored_view() {
        // Risen in B's region 11 (region index 1): at load only that region's slot is open, so his
        // zone crosses from region 10 into it, and +0x400 = 1.
        let mut m = risen_rebel(RegionId(11));
        assert!(east(&m, GA) > 40.0, "{}", east(&m, GA));
        let k = m.commander(GA).unwrap();
        // In A's turn the stored view is A's, which has a record: the scope floods with A's modes and
        // his own flags (his home region's slot).
        reflood_in_turn_of(&mut m, A);
        assert!(east(&m, GA) > 40.0, "{}", east(&m, GA));
        let (modes, flags) = m.flood_view(map(&m), &k, &m.region_owners(), m.grid_state().unwrap());
        assert_eq!((modes, flags.unwrap()), (A, vec![false, true, false, false]));
        // In the rebels' turn the stored view has no record and +0x400 is set: the flood runs on the
        // layers as the rebels' faction view left them, every single region closed.
        reflood_in_turn_of(&mut m, REBELS);
        assert!(east(&m, GA) < 40.0, "{}", east(&m, GA));
        let (modes, flags) = m.flood_view(map(&m), &k, &m.region_owners(), m.grid_state().unwrap());
        assert_eq!((modes, flags.unwrap()), (REBELS, vec![false, false, false, true]));
        // His own order stores his view again (`0x00B077B0`): his home region's slot only.
        m.store_rebel_view(ForceId(1000));
        m.rebuild_zone(GA);
        assert!(east(&m, GA) > 40.0, "{}", east(&m, GA));
        let (modes, flags) = m.flood_view(map(&m), &k, &m.region_owners(), m.grid_state().unwrap());
        assert_eq!((modes, flags.unwrap()), (REBELS, vec![false, true, false, false]));
    }

    #[test]
    fn a_rebel_floods_in_his_own_scope_while_the_single_region_field_is_0() {
        // Risen in A's region 10 (region index 0): +0x400 stays 0, so the rebels' stored faction view
        // (every single region closed) is not read; the scope gives his home region's slot.
        let mut m = risen_rebel(RegionId(10));
        reflood_in_turn_of(&mut m, REBELS);
        assert_eq!(m.grid_state().unwrap().region, 0);
        assert!(!zone(&m, GA).is_empty() && east(&m, GA) < 40.0, "{}", east(&m, GA));
        let k = m.commander(GA).unwrap();
        let (modes, flags) = m.flood_view(map(&m), &k, &m.region_owners(), m.grid_state().unwrap());
        assert_eq!((modes, flags.unwrap()), (REBELS, vec![true, false, false, false]));
    }

    /// Campaign +0x39 is clear during the load pass: each commander floods with his own faction's
    /// modes there, and with the stored view's after it.
    #[test]
    fn the_load_pass_floods_with_each_commanders_own_modes_and_later_floods_with_the_views() {
        let m = model(true);
        let k = m.commander(GB).unwrap();
        let mut load = m.grid_state().unwrap();
        load.read_view = false;
        assert_eq!(m.flood_view(map(&m), &k, &m.region_owners(), load).0, B);
        assert_eq!(m.flood_view(map(&m), &k, &m.region_owners(), m.grid_state().unwrap()).0, A);
    }

    /// A commander of a faction with a record flooded while the stored view is a rebel army's (an
    /// order's) floods on that view's layers: its modes and its region's slots only.
    #[test]
    fn any_commander_flooded_after_a_rebels_order_gets_the_rebel_view() {
        let mut m = risen_rebel(RegionId(11));
        reflood_in_turn_of(&mut m, REBELS);
        m.store_rebel_view(ForceId(1000));
        let k = m.commander(GB).unwrap();
        let (modes, flags) = m.flood_view(map(&m), &k, &m.region_owners(), m.grid_state().unwrap());
        assert_eq!((modes, flags.unwrap()), (REBELS, vec![false, true, false, false]));
    }

    #[test]
    fn a_rebels_force_order_stores_his_view_and_a_failed_one_keeps_it() {
        let mut m = risen_rebel(RegionId(11));
        m.turn.started = true;
        m.turn.in_turn = true;
        m.turn.current = Some(A);
        m.store_acting_view();
        let before = m.grid_state();
        // A faction with a record stores nothing on its orders.
        m.store_rebel_view(m.force_of(GB).unwrap());
        assert_eq!(m.grid_state(), before);
        // Not the rebels' turn: the order fails, and his view, stored before the order is issued,
        // stays (`0x00A645C0` never puts the old view back).
        assert!(m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: at(30, 21) }).is_err());
        let g = m.grid_state().unwrap();
        assert_eq!((g.view, g.region), (Some(GridView::Rebel(REBELS)), 1));
        // In the rebels' turn his order stores his view (`0x00AAF550` → `0x00B077B0`) before he moves.
        m.turn.current = Some(REBELS);
        m.store_acting_view();
        assert!(m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: at(30, 21) }).is_ok());
        let g = m.grid_state().unwrap();
        assert_eq!((g.view, g.region), (Some(GridView::Rebel(REBELS)), 1));
    }

    /// A rebel army risen in a region the grid does not hold, in a model with no acting faction yet.
    fn homeless_rebel() -> CampaignModel {
        let mut m = border_world();
        m.world.forces.get_mut(&ForceId(1000)).unwrap().faction = REBELS;
        m.world.characters.get_mut(&GA).unwrap().faction = REBELS;
        m.world.region_risen_armies.insert(RegionId(999), vec![ForceId(1000)]);
        m.turn.current = None;
        m.attach_terrain(region_terrain());
        m
    }

    #[test]
    fn a_rebel_with_no_home_region_on_the_grid_stores_no_view_at_load_and_at_his_order() {
        // The load pass: his view is not stored (and the previous one is not kept), he still floods.
        let mut m = homeless_rebel();
        assert_eq!(m.grid_state().unwrap().view, None);
        assert!(!zone(&m, GA).is_empty());
        // His order clears a view stored before it, the same way.
        reflood_in_turn_of(&mut m, A);
        assert_eq!(m.grid_state().unwrap().view, Some(GridView::Faction(A)));
        m.store_rebel_view(ForceId(1000));
        assert_eq!(m.grid_state().unwrap().view, None);
    }

    /// [`CampaignModel::flood_view`] on grid states made by hand: the view faction, then the layers
    /// the stored view left, for the rebel army risen in region index 1.
    #[test]
    fn the_flood_view_choice_follows_the_stored_view() {
        let m = risen_rebel(RegionId(11));
        let rebel = m.commander(GA).unwrap();
        let other = m.commander(GB).unwrap();
        let view = |c: &Commander, grid: GridState| {
            let (modes, flags) = m.flood_view(map(&m), c, &m.region_owners(), grid);
            (modes, flags.unwrap())
        };
        let state = |view, region, read_view| GridState { view, region, read_view };
        // Before campaign +0x39: the commander's own faction, whatever is stored.
        assert_eq!(view(&other, state(Some(GridView::Rebel(REBELS)), 1, false)).0, B);
        // A rebel view with +0x400 set: its modes and only its region's slots, for any commander.
        assert_eq!(view(&other, state(Some(GridView::Rebel(REBELS)), 1, true)), (REBELS, vec![false, true, false, false]));
        // A rebel faction view with +0x400 set: its modes and every single region closed.
        assert_eq!(view(&other, state(Some(GridView::Faction(REBELS)), 1, true)), (REBELS, vec![false, false, false, true]));
        // A view faction with a record: its modes with the commander's own flags.
        assert_eq!(view(&rebel, state(Some(GridView::Faction(A)), 1, true)), (A, vec![false, true, false, false]));
        // No view stored (only after the load-pass fallback): the commander's own faction and gate.
        assert_eq!(view(&rebel, state(None, 1, true)), (REBELS, vec![false, true, false, false]));
        // +0x400 still 0: the scope, whatever the rebel view.
        assert_eq!(view(&rebel, state(Some(GridView::Rebel(REBELS)), 0, true)), (REBELS, vec![false, true, false, false]));
    }
}
