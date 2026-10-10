//! The campaign world: factions, regions, characters and military forces.
//!
//! The layout mirrors the original's `CAMPAIGN_MODEL` v10 → `WORLD` tree (W3 §3, CONFIRMED
//! structure). Only the fields that the current rules need are modelled; everything else in the
//! ESF records (traits, ancillaries, technologies, population classes, ...) is still TODO.
//!
//! Every collection is a `BTreeMap` keyed by the original's 32-bit id, so iteration is always in
//! id order and the simulation stays deterministic. (A `HashMap` would iterate in a random order.)

use std::collections::BTreeMap;
use std::sync::Arc;

use super::battles::PendingBattle;
use super::ids::{CharacterId, FactionId, FortId, ForceId, RecruitmentItemId, RegionId, UnitId};
use super::pathing::PathGrid;
use super::rules::{CampaignRules, ForceCaps};
use super::turn::TurnState;
use crate::calendar::Calendar;
use crate::fixed::Fixed20;
use crate::fnv::Fnv64;
use crate::rng::CaRng;

/// The map's movement grid, shared and immutable. Compared by pointer (two models built from the
/// same map share one grid), so `CampaignModel` equality stays cheap.
#[derive(Debug, Clone)]
pub struct Terrain(pub Arc<PathGrid>);

impl PartialEq for Terrain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The whole campaign model: the original's `CAMPAIGN_MODEL` (W3 §3, CONFIRMED child order
/// `CAMPAIGN_MAP_DATA`, u32, `RandSeed`, `CAMPAIGN_CALENDAR`, `WORLD`, ...).
///
/// `rules` (game data from the DB) and `terrain` (the map's movement grid) are our own additions:
/// they are data, not state, and are neither saved nor hashed.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CampaignModel {
    /// `CAMPAIGN_CALENDAR` (W3 §3.1, CONFIRMED).
    pub calendar: Calendar,
    /// The campaign's own RNG owner (`RandSeed`, W3 §3; LCG CONFIRMED in W1 §12.1). Autoresolve
    /// draws from it.
    pub rng: CaRng,
    /// `WORLD` (W3 §3, CONFIRMED).
    pub world: World,
    /// Whose turn it is and what is left to process (see [`turn`](super::turn)).
    pub turn: TurnState,
    /// Units per army and per navy (`CAMPAIGN_MODEL` #23 / #24, [`ForceCaps`]): read from the file,
    /// set by [`ForceCaps::new_campaign`] when a new campaign starts, saved (an own save from before
    /// the field reads 20 / 20, the caps it played with).
    #[cfg_attr(feature = "serde", serde(default))]
    pub force_caps: ForceCaps,
    /// The deal inflation factor and the net it is measured against (`CAMPAIGN_MODEL` #21 / #22,
    /// campaign `+0x1010` / `+0x1014`, [`super::deal_value::DealInflation`]): read from the file,
    /// updated at each round end ([`super::deal_value::DealInflation::round_end`]), saved.
    #[cfg_attr(feature = "serde", serde(default))]
    pub deal_inflation: super::deal_value::DealInflation,
    /// The values mods' rules keep ([`super::mod_state`]; DESIGN.md §3.3.1): empty in vanilla, saved
    /// and hashed (an own save from before the field reads it empty).
    #[cfg_attr(feature = "serde", serde(default))]
    pub mod_state: super::mod_state::ModState,
    /// A battle waiting to be fought (the original's `PENDING_BATTLE` record, W3 §2; its content
    /// is not decoded). Set by an attack, cleared by autoresolve or a battle result.
    pub pending_battle: Option<PendingBattle>,
    /// A capture waiting for the human's occupy / loot / liberate choice (see [`capture`](super::capture)). Set
    /// when a human faction takes a settlement by force, cleared by `ChooseCapture` (or settled as an occupation
    /// at the end of the turn).
    pub pending_capture: Option<super::capture::CapturePreview>,
    /// Game data from the DB (see [`CampaignRules`]).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub rules: Arc<CampaignRules>,
    /// The map's movement grid, if the map was loaded. Without it, moves are straight lines
    /// (used by unit tests).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub terrain: Option<Terrain>,
    /// The last autoresolve result (for the battle report and the log). Not state: not saved, not
    /// hashed.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub last_autoresolve: Option<super::autoresolve::ArOutcome>,
    /// The RNGs of the `effect.trait` / `effect.ancillary` script bindings (see
    /// [`ScriptRngs`](super::characters::ScriptRngs)). Hashed, not saved (the original keeps them
    /// process-wide).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub script_rngs: super::characters::ScriptRngs,
    /// The campaign negotiation slot (campaign +0xF9C) and its begin / end counts, see
    /// [`super::negotiation::Negotiations`]. Not saved, not hashed.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub negotiations: super::negotiation::Negotiations,
}

/// Equality over everything but [`CampaignModel::negotiations`]: the negotiation slot is a UI
/// session, neither saved nor hashed. Every other field is compared, including `rules`, `terrain` and
/// `last_autoresolve`, which are also neither saved nor hashed.
impl PartialEq for CampaignModel {
    fn eq(&self, other: &Self) -> bool {
        // Destructured so a new field must be placed here on purpose.
        let CampaignModel {
            calendar,
            rng,
            world,
            turn,
            force_caps,
            deal_inflation,
            pending_battle,
            pending_capture,
            rules,
            terrain,
            last_autoresolve,
            script_rngs,
            mod_state,
            negotiations: _,
        } = self;
        *calendar == other.calendar
            && *rng == other.rng
            && *world == other.world
            && *turn == other.turn
            && *force_caps == other.force_caps
            && *deal_inflation == other.deal_inflation
            && *pending_battle == other.pending_battle
            && *pending_capture == other.pending_capture
            && *rules == other.rules
            && *terrain == other.terrain
            && *last_autoresolve == other.last_autoresolve
            && *script_rngs == other.script_rngs
            && *mod_state == other.mod_state
    }
}

impl CampaignModel {
    /// Builds a model from its parts with empty rules (no DB data) and no terrain. The turn has
    /// not started yet (see [`CampaignModel::start_campaign`]).
    pub fn new(calendar: Calendar, rng: CaRng, world: World) -> Self {
        CampaignModel {
            calendar,
            rng,
            world,
            turn: TurnState::default(),
            force_caps: ForceCaps::default(),
            deal_inflation: super::deal_value::DealInflation::default(),
            mod_state: super::mod_state::ModState::default(),
            pending_battle: None,
            pending_capture: None,
            rules: Arc::new(CampaignRules::default()),
            terrain: None,
            last_autoresolve: None,
            script_rngs: super::characters::ScriptRngs::default(),
            negotiations: super::negotiation::Negotiations::default(),
        }
    }

    /// The most units an army (`is_navy` false) or a navy may hold: the campaign's
    /// [`CampaignModel::force_caps`] (the army's / navy's capacity virtual `+0x48`, CONFIRMED).
    /// Every unit-count check of the model and the AI asks this.
    pub fn max_units(&self, is_navy: bool) -> usize {
        // u32 -> usize never truncates on the 32- and 64-bit targets we build for.
        (if is_navy { self.force_caps.navy } else { self.force_caps.army }) as usize
    }

    /// A deterministic 64-bit hash (FNV-1a) of the whole campaign state, for desync checks and
    /// tests (our own tool, like `Battle::state_hash`). Collections are hashed in id order.
    /// Rules and terrain are not hashed (they are game data, not state).
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv64::new();
        let c = &self.calendar;
        h.u32(c.turns_per_year);
        h.u32(c.turn_in_year);
        h.u32(c.date.year);
        h.u32(c.date.season);
        h.u32(c.date.month);
        h.u32(c.date.half);
        h.u32(c.turns_elapsed);
        h.u32(self.rng.state);
        h.u32(self.force_caps.army);
        h.u32(self.force_caps.navy);
        h.u32(self.deal_inflation.first);
        h.u32(self.deal_inflation.factor.to_bits());
        h.u32(self.script_rngs.trait_rng.state);
        h.u32(self.script_rngs.ancillary_rng.state);
        // Only when a mod keeps values, so a vanilla campaign hashes as before.
        if !self.mod_state.is_empty() {
            self.mod_state.hash_into(&mut h);
        }
        self.turn.hash_into(&mut h);
        match &self.pending_battle {
            None => h.u32(0),
            Some(b) => {
                h.u32(1);
                h.u32(b.attacker.raw());
                h.u32(b.defenders.len() as u32);
                for d in &b.defenders {
                    h.u32(d.raw());
                }
                h.u32(b.settlement.map_or(u32::MAX, RegionId::raw));
            }
        }
        match &self.pending_capture {
            None => h.u32(0),
            Some(p) => {
                h.u32(1);
                h.u32(p.region.raw());
                h.i32(p.faction.raw());
            }
        }

        let w = &self.world;
        h.u32(w.turn_order.len() as u32);
        for f in &w.turn_order {
            h.i32(f.raw());
        }
        h.u32(w.factions.len() as u32);
        for f in w.factions.values() {
            h.i32(f.id.raw());
            h.str(&f.key);
            h.i32(f.treasury);
            h.u32(f.government as u32);
            h.str(&f.government_key);
            h.str(&f.tax_lower);
            h.str(&f.tax_upper);
            h.u32(f.diplomacy.len() as u32);
            for (other, stance) in &f.diplomacy {
                h.i32(other.raw());
                h.u32(*stance as u32);
            }
        }
        h.u32(w.bankrupt_turns.len() as u32);
        for (f, n) in &w.bankrupt_turns {
            h.i32(f.raw());
            h.u32(*n);
        }
        h.u32(w.economy_history.len() as u32);
        for (f, records) in &w.economy_history {
            h.i32(f.raw());
            h.u32(records.len() as u32);
            for v in records.iter().flatten() {
                h.i32(*v);
            }
        }
        // The relationship records the diplomacy rules change (attitude factors and counters).
        h.u32(w.relationships.len() as u32);
        for ((a, b), r) in &w.relationships {
            h.i32(a.raw());
            h.i32(b.raw());
            for f in &r.attitudes {
                h.i32(f.value);
                h.i32(f.drift);
            }
            h.u32(r.trade_agreement as u32);
            h.i32(r.military_access_turns);
            h.u32(r.friendship_turns);
            h.u32(r.alliance_commitment_turns);
            h.u32(r.payments.len() as u32);
        }
        h.u32(w.ship_states.len() as u32);
        for (u, s) in &w.ship_states {
            h.i32(u.raw());
            for d in s.damage {
                h.u32(d.to_bits());
            }
            for c in s.crews {
                h.i32(c);
            }
            h.u32(s.guns);
            h.u32(s.sunk as u32);
        }
        for m in [&w.treaty_breaks, &w.alliances_broken, &w.access_cancel_marks] {
            h.u32(m.len() as u32);
            for (f, n) in m {
                h.i32(f.raw());
                h.u32(*n);
            }
        }
        // The scripts' restricted levels decide what may be built.
        h.u32(w.restricted_buildings.len() as u32);
        for k in &w.restricted_buildings {
            h.str(k);
        }
        // The forts are static file data (`World::forts`; nothing changes one), hashed so a
        // desync check still notices a file that loaded a different set.
        h.u32(w.forts.len() as u32);
        for f in w.forts.values() {
            h.u32(f.id.raw());
            h.u32(f.region.raw());
            h.str(&f.key);
            match f.position {
                Some((x, z)) => {
                    h.u32(1);
                    h.i32(x.raw());
                    h.i32(z.raw());
                }
                None => h.u32(0),
            }
        }
        h.u32(w.trade_accumulated.len() as u32);
        for ((a, b), v) in &w.trade_accumulated {
            h.i32(a.raw());
            h.i32(b.raw());
            h.i32(*v);
        }
        h.u32(w.trade_paths.len() as u32);
        for ((a, b), paths) in &w.trade_paths {
            h.i32(a.raw());
            h.i32(b.raw());
            h.u32(paths.len() as u32);
            for p in paths {
                h.u32(p.waypoints.len() as u32);
                for wp in &p.waypoints {
                    h.u32(wp.region.0);
                    h.u32(wp.from);
                    h.u32(wp.to);
                    h.u32(wp.sea as u32);
                }
                h.u32(p.volumes.len() as u32);
                for v in &p.volumes {
                    h.u32(*v);
                }
            }
        }
        h.u32(w.domestic_trade.len() as u32);
        for (f, paths) in &w.domestic_trade {
            h.i32(f.raw());
            for p in paths {
                h.u32(p.waypoints.len() as u32);
                for v in &p.volumes {
                    h.u32(*v);
                }
            }
        }
        for p in w.commodity_market.factors.iter() {
            h.u32(p.to_bits());
        }
        for v in w.commodity_market.previous.iter().chain(&w.commodity_market.previous2).chain(&w.commodity_market.trend) {
            h.u32(*v);
        }
        h.u32(w.commodity_prices.len() as u32);
        for p in &w.commodity_prices {
            h.u32(*p);
        }
        h.u32(w.regions.len() as u32);
        let building = |h: &mut Fnv64, b: &Option<BuildingRef>| match b {
            None => h.u32(0),
            Some(b) => {
                h.u32(1);
                h.str(&b.level_key);
                h.u32(b.health);
            }
        };
        for r in w.regions.values() {
            h.u32(r.id.raw());
            h.str(&r.key);
            h.i32(r.owner.raw());
            h.str(&r.settlement.key);
            h.i32(r.settlement.position.0.raw());
            h.i32(r.settlement.position.1.raw());
            h.u32(r.slots.len() as u32);
            for slot in &r.slots {
                h.str(&slot.key);
                h.str(&slot.slot_type);
                building(&mut h, &slot.building);
            }
            building(&mut h, &r.road);
            building(&mut h, &r.fortification);
            h.u32(r.population);
            let p = &r.population_state;
            for f in p.factors {
                h.u32(f.to_bits());
            }
            h.u32(p.capacity);
            h.u32(p.base_capacity);
            h.u32(p.growth.to_bits());
            h.u32(p.trend);
            h.u32(u32::from(p.overcrowded));
            h.i32(p.migrants);
            h.u32(r.religions.len() as u32);
            for (k, s) in &r.religions {
                h.str(k);
                h.u32(s.to_bits());
            }
            h.u32(r.base_gdp);
            h.u32(r.gdp);
            h.i32(r.wealth_growth_offset);
            h.i32(r.discontent_growth);
            h.u32(r.town_wealth);
            h.i32(r.town_wealth_growth);
            h.u32(u32::from(r.tax_exempt));
            h.u32(r.garrison.map_or(u32::MAX, ForceId::raw));
            h.u32(r.fleet.map_or(u32::MAX, ForceId::raw));
            h.u32(r.recruitment_queue.len() as u32);
            for item in &r.recruitment_queue {
                h.i32(item.id.raw());
                h.str(&item.unit_key);
                h.u32(item.turns_remaining);
                h.i32(item.cost);
                // A tag byte, then the commander's id: no sentinel value.
                match item.target {
                    None => h.bytes(&[0]),
                    Some(c) => {
                        h.bytes(&[1]);
                        h.i32(c.raw());
                    }
                }
            }
            h.u32(r.construction.len() as u32);
            for item in &r.construction {
                // A tag byte, then the full-width index of a list slot: no sentinel values.
                match item.slot {
                    SlotRef::Slot(i) => {
                        h.bytes(&[0]);
                        h.bytes(&(i as u64).to_le_bytes());
                    }
                    SlotRef::Walls => h.bytes(&[1]),
                    SlotRef::Road => h.bytes(&[2]),
                }
                h.str(&item.level_key);
                h.u32(item.turns_remaining);
                h.i32(item.cost);
            }
        }
        h.u32(w.characters.len() as u32);
        for ch in w.characters.values() {
            h.i32(ch.id.raw());
            h.i32(ch.faction.raw());
            h.u32(ch.kind as u32);
            h.i32(ch.position.0.raw());
            h.i32(ch.position.1.raw());
            h.i32(ch.movement_points);
            h.i32(ch.max_movement_points);
            h.i32(ch.base_movement_points);
            h.u32(ch.garrisoned_in.map_or(u32::MAX, RegionId::raw));
        }
        h.u32(w.forces.len() as u32);
        for fo in w.forces.values() {
            h.u32(fo.id.raw());
            h.i32(fo.faction.raw());
            h.i32(fo.commander.map_or(i32::MIN, CharacterId::raw));
            h.u32(fo.is_navy as u32);
            h.u32(fo.units.len() as u32);
            for u in &fo.units {
                h.i32(u.id.raw());
                h.str(&u.unit_key);
                h.u32(u.men);
                h.u32(u.max_men);
                h.i32(u.character.map_or(i32::MIN, CharacterId::raw));
            }
        }
        h.u32(w.embarked.len() as u32);
        for (army, navy) in &w.embarked {
            h.u32(army.raw());
            h.u32(navy.raw());
        }
        h.u32(w.deal_regions_received.len() as u32);
        for (f, n) in &w.deal_regions_received {
            h.i32(f.raw());
            h.u32(*n);
        }
        h.finish()
    }

    /// The faction with this key.
    pub fn faction_by_key(&self, key: &str) -> Option<&Faction> {
        self.world.factions.values().find(|f| f.key == key)
    }

    /// The map position of a force: its commander's position, else (a garrison without a
    /// commander) the settlement of the region it garrisons.
    pub fn force_position(&self, force: ForceId) -> Option<(Fixed20, Fixed20)> {
        let f = self.world.forces.get(&force)?;
        if let Some(c) = f.commander.and_then(|c| self.world.characters.get(&c)) {
            return Some(c.position);
        }
        self.world.regions.values().find(|r| r.garrison == Some(force)).map(|r| r.settlement.position)
    }

    /// The force a character commands, if any.
    pub fn force_of(&self, character: CharacterId) -> Option<ForceId> {
        self.world.forces.values().find(|f| f.commander == Some(character)).map(|f| f.id)
    }
}

/// The original's `WORLD` record (W3 §3, CONFIRMED). Every collection is keyed by id.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct World {
    /// `FACTION` records (W3 §3.3).
    pub factions: BTreeMap<FactionId, Faction>,
    /// The order factions take their turns: the order of `WORLD/FACTION_ARRAY` (INFERRED: the
    /// player's faction is first in every shipped start position, e.g. france in eur), then the
    /// rebel faction. Factions missing here take their turns after the listed ones, in id order.
    pub turn_order: Vec<FactionId>,
    /// `REGION` records (W3 §3.7).
    pub regions: BTreeMap<RegionId, Region>,
    /// The forts standing in regions: each `REGION`'s `FORT_ARRAY` items, flattened here with
    /// [`Fort::region`] (like characters and forces, since a fort is a map object of its own and the
    /// original registers it as a garrison residence beside the region's own).
    ///
    /// **Empty in every shipped file**: `FORT_ARRAY` is present in all eight start positions and
    /// all ten vanilla saves and holds **0 items** everywhere (CONFIRMED by reading the install's
    /// `campaigns/*/startpos.esf` and the vanilla saves; `esf_find FORT_ARRAY` prints the path of
    /// every one). Nothing in the model creates a fort either -- `BuildFort` /
    /// `UpgradeFort` address the settlement's `FORTIFICATION_SLOT` building, which is
    /// [`Region::fortification`] and has no `FORT_ARRAY` entry. So this map only fills from a file
    /// that already has forts.
    ///
    /// Hashed by [`World::state_hash`] (nothing changes a fort, so a differing hash means the file
    /// loaded a different set), but no rule writes it yet.
    pub forts: BTreeMap<FortId, Fort>,
    /// `CHARACTER` records (W3 §3.5). In the ESF they live inside each faction's
    /// `CHARACTER_ARRAY`; here they are flattened into one map, with [`Character::faction`].
    pub characters: BTreeMap<CharacterId, Character>,
    /// `ARMY` / `NAVY` → `MILITARY_FORCE` records (W3 §3.6). In the ESF they live inside each
    /// faction's `ARMY_ARRAY`; here they are flattened, with [`MilitaryForce::faction`].
    pub forces: BTreeMap<ForceId, MilitaryForce>,
    /// `CHARACTER_DETAILS` of each character (names, portrait, traits, ancillaries, post), see
    /// [`details`](super::details). Not part of `state_hash` (data the rules do not use yet).
    pub character_details: super::details::CharacterDetailsMap,
    /// Government, capital and religion of each faction (see [`details`](super::details)).
    pub faction_details: super::details::FactionDetailsMap,
    /// The full `DIPLOMACY_RELATIONSHIP` records by (owner, target) (see
    /// [`details`](super::details)); the stances are in [`Faction::diplomacy`].
    pub relationships: super::details::RelationshipMap,
    /// The accumulated value of each trade route, by (exporter, importer): it grows every round
    /// by `trade_route_value_accumulator_proportion` of the route's value (0x00B05CC0, CONFIRMED).
    /// Loaded from the saved routes (`ntw_campaign::trade::read_accumulated`, summed per pair).
    pub trade_accumulated: BTreeMap<(FactionId, FactionId), i32>,
    /// The international routes of each (exporter, importer) pair, with their paths and commodity
    /// volumes (see [`trade`](super::trade)). Loaded from the file; a pair with a trade agreement
    /// but no entry gets a route built over the network ([`CampaignModel::build_trade_route`];
    /// PROVISIONAL candidate set) and its volumes from the supply split.
    pub trade_paths: BTreeMap<(FactionId, FactionId), Vec<super::trade::TradePath>>,
    /// Commodity prices in `COMMODITIES_ORDER` (`CAMPAIGN_TRADE_MANAGER` u32[8] #4, CONFIRMED by
    /// the stored route values). Kept as loaded (how the original moves them is not read).
    pub commodity_prices: Vec<u32>,
    /// Each faction's domestic routes (`DOMESTIC_TRADE_ROUTES`): from the off-map trade nodes to its
    /// ports, with the commodity volumes its trade ships bring home (see [`trade`](super::trade)).
    pub domestic_trade: BTreeMap<FactionId, Vec<super::trade::TradePath>>,
    /// Commodity keys in `COMMODITIES_ORDER` (index = commodity number). Static data.
    pub commodity_keys: Vec<String>,
    /// The price state of `CAMPAIGN_TRADE_MANAGER` (#2 initial prices, #3 price factors f32, #5 / #6
    /// the two previous prices, #7 trends; #4 is [`World::commodity_prices`]). State: changed by
    /// [`CampaignModel::update_commodity_prices`](super::CampaignModel::update_commodity_prices).
    pub commodity_market: super::trade::CommodityMarket,
    /// The off-map trade nodes (see [`trade`](super::trade)). Static data, not hashed.
    pub trade_nodes: Vec<super::trade::TradeNode>,
    /// The static trade network legs (`TRADE_ROUTES`). Static data, not hashed.
    pub trade_network: Vec<super::trade::TradeLeg>,
    /// Network node → (region, is a port) for ports and settlements. Static data, not hashed.
    pub trade_node_regions: BTreeMap<u32, (RegionId, bool)>,
    /// Land neighbours of each region (game region +0x20/+0x24): the regions whose map outlines touch
    /// (`regions.esf` outline `connectivity`). Empty when the map is not attached. Static data, not
    /// hashed.
    pub region_neighbours: BTreeMap<RegionId, Vec<RegionId>>,
    /// Bankrupt turns in a row of each faction (the economics object's +0x460, CONFIRMED); a faction
    /// that can pay is not listed.
    pub bankrupt_turns: BTreeMap<FactionId, u32>,
    /// Each faction's economics history (`FACTION_ECONOMICS` #0, the economics object's ring of
    /// [`ECONOMY_HISTORY_LEN`] records of 25 integer categories at `+0x04`, count `+0x3EC`, current
    /// index `+0x3F0`), oldest first as the saver `0x00BD46E0` writes it (CONFIRMED). Income is
    /// categories 5..11 (5 taxes, 7 trade, 11 other), expenses 18..24 (19 land, 20 naval upkeep);
    /// the rest are one-off spending and refunds (CAMPAIGN_FIDELITY.md "FACTION_ECONOMICS").
    /// Loaded and saved; [`super::economy::settle_round`] adds a record each round end. A faction
    /// without an entry has no history.
    #[cfg_attr(feature = "serde", serde(default))]
    pub economy_history: BTreeMap<FactionId, Vec<EconomyRecord>>,
    /// Each faction's diplomacy manager counters (CONFIRMED uses, not loaded from the file yet: the
    /// `DIPLOMACY_MANAGER` fields are not read): +0x1C the treaties it broke during friendship (the
    /// backstabbing count, `0x00B0E420`) ...
    pub treaty_breaks: BTreeMap<FactionId, u32>,
    /// Each ship's damage state (`NAVAL_UNIT/SHIP_DAMAGE_INFO`, see [`super::naval::ShipState`]), by unit. A ship
    /// without an entry is undamaged with its type's crews.
    pub ship_states: BTreeMap<super::ids::UnitId, super::naval::ShipState>,
    /// ... +0x14 the alliances it broke while committed (`0x00B13840`) ...
    pub alliances_broken: BTreeMap<FactionId, u32>,
    /// ... +0x20 a third of the grievance of each military access it withdrew (`0x00B67B20`), −1 a turn
    /// (`0x00B29100`).
    pub access_cancel_marks: BTreeMap<FactionId, u32>,
    /// `REGION` #24 of each region: its rebel faction key (CONFIRMED position). The capture preview
    /// offers to liberate the campaign faction with this key (`0x00B14930`; see
    /// [`capture`](super::capture)). Static data, not hashed.
    pub region_rebel_factions: BTreeMap<RegionId, String>,
    /// The script missions of each faction (see [`CampaignMission`](super::details::CampaignMission)).
    /// Not part of `state_hash`; no rule runs them yet (PROVISIONAL: loaded for the UI and scripts).
    pub missions: super::details::MissionMap,
    /// The building levels the scripts restricted (`add_restricted_building_level_record` /
    /// `remove_restricted_building_level_record`): the campaign's one restricted set, shared by every
    /// faction (faction `+0xA8` → `+0x8` → `+0xFA8`, scanned by `0x009CDA90`, CONFIRMED). The
    /// permission test ([`CampaignModel::building_permitted`]) and the map fort upgrade
    /// ([`CampaignModel::map_fort_next_level`]) reject these levels, so the panels, the commands
    /// and the AI all read this one list. Saved in `EPISODIC_RESTRICTIONS` (`ntw_campaign`).
    pub restricted_buildings: std::collections::BTreeSet<String>,
    /// The next id [`World::alloc_id`] hands out. The original keeps one global id → object map
    /// for every kind of object (SAVE_COMPAT.md §3, CONFIRMED in the exe), so new ids must not
    /// collide with any id in the loaded file: the loader sets this above every integer of the
    /// file (`ntw_campaign`). 0 = not set (then `alloc_id` starts above the ids in the model).
    pub next_id: u32,
    /// Where each loaded recruitment item came from in the loaded file (see [`RecruitmentSource`]):
    /// the save writer keeps exactly that record for it (`ntw_campaign` `save.rs`). Items made since
    /// have none and are written fresh. Not saved (the next load links again). A link outlives its
    /// item harmlessly: the writer reads only queued items' links, and a new item takes no id the
    /// file holds below `0x7fff_0000` ([`World::alloc_id`] starts above them; a file id at or above
    /// that it reaches only after handing out every id in between, see its doc). A link to a record
    /// that no longer matches is written fresh and logged once per save (`ntw_campaign`).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub recruitment_sources: BTreeMap<RecruitmentItemId, RecruitmentSource>,
    /// Armies aboard a navy: army → the navy carrying it (see [`super::embark`]). An embarked
    /// army's commander stands at the navy's position. The original saves the pair as NAVY #4 (army
    /// aboard) and ARMY #7 (carrying fleet), CONFIRMED (PATHFINDING_PORTS.md §9.3); the loader
    /// (`ntw_campaign`) reads those, and falls back to "army commander where its navy is" for our
    /// own saves, whose writer does not store the link yet (PROVISIONAL).
    pub embarked: BTreeMap<ForceId, ForceId>,
    /// Agents that used their action this turn (character +0x4CC, set by every agent action and
    /// cleared when the faction's characters start their turn; CONFIRMED use, CHARACTERS_FIDELITY.md
    /// §7). Not saved (UNKNOWN whether the original saves it).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub agents_acted: std::collections::BTreeSet<CharacterId>,
    /// Forces sabotaged by an agent (force +0xE8, set by `0x008EB660`): at its commander's next turn
    /// start his action points are set to 0 and the mark is cleared (`0x008F2290` → `0x00A2A140`). Not
    /// saved (CONFIRMED: the army loader `0x00870FD0` clears it).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub sabotaged: std::collections::BTreeSet<ForceId>,
    /// The sight cell grid (`super::visibility`), from the loaded file; `None` when unknown.
    pub sight_grid: Option<super::visibility::SightGrid>,
    /// Each faction's shroud (`FACTION` `CAMPAIGN_SHROUD`; only factions that have one).
    pub shrouds: BTreeMap<FactionId, super::visibility::Shroud>,
    /// The saved sight shape of each region (`REGION` `LINE_OF_SIGHT`, where computed).
    pub region_sight: super::visibility::RegionSight,
    /// The trade route segments' sight (`TRADE_SEGMENTS[]`, in file order).
    pub trade_sight: Vec<super::visibility::TradeSegmentSight>,
    /// Each character's sight radius (character +0x2EC, `CHARACTER` #17 f32, CONFIRMED by the loader
    /// `0x00991520`), as loaded and as [`CampaignModel::update_sight_radius`] sets it; a character not
    /// listed sees with his type's radius ([`CampaignModel::sight_radius`]).
    pub sight_radius: BTreeMap<CharacterId, f32>,
    /// `CAMPAIGN_MODEL` `HISTORICAL_CHARACTER_MANAGER` `CREATED_CHARACTER_ARRAY[]`: the
    /// `historical_characters` keys already made into characters (CONFIRMED: the condition
    /// `0x0089B830` refuses a key found in this sorted list; 18 in the vanilla Peninsular saves).
    /// Kept sorted.
    pub historical_created: Vec<String>,
    /// Two episodic-scripting switches of `CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS` (#20 and #22,
    /// CONFIRMED positions): `force_assassination_success_for_human` (+0xEA, set by `0x0097A620`)
    /// and `force_garrison_infiltration_success_for_human` (+0xEC, `0x0097A8F0`): a human agent's
    /// assassination / settlement spying gets outcome 1 without a roll (`0x0094ACB0`,
    /// `0x0094CCB0`). Both false in every vanilla save; only the tutorial scripts set them.
    pub force_success_for_human: (bool, bool),
    /// The human factions' spy-network sight (the faction sight object +0x800, its +0x20 list of
    /// shapes; CONFIRMED structure, CHARACTERS_FIDELITY.md §10): at the faction's turn start
    /// (`0x008F2480`, humans only) the sight disc of every character with `subterfuge` > 0 who
    /// has not moved for 3 turns or more (`0x008B4120`) is listed here (position, radius) and
    /// counts as a sight source until the next turn start. Not saved (rebuilt each turn start).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub network_sight: NetworkSight,
    /// Each faction's campaign AI manager and personality keys (the faction's `+0x82C` / `+0x838`,
    /// CONFIRMED, `analysis/ai/AI_RESEARCH.md` §2.3), filled by the campaign source (the original's
    /// importer reads them from the `FACTION` record). A faction without an entry uses the AI's
    /// PROVISIONAL naming rule (`ntw_ai::campaign::FactionAiConfig::resolve`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub ai_keys: BTreeMap<FactionId, FactionAiKeys>,
    /// Each faction's name allocators in save order (`NAME_ALLOCATION_DETAILS`, [`super::names`]):
    /// the decks new characters' names are drawn from. Filled by the campaign source.
    #[cfg_attr(feature = "serde", serde(default))]
    pub name_allocators: BTreeMap<FactionId, Vec<super::names::NameAllocator>>,
    /// The portrait allocator, one entry per culture in save order (`PORTRAIT_ALLOCATOR`,
    /// [`super::portraits`]): the decks new characters' portraits are drawn from. Filled by the
    /// campaign source; saved with the model, deck seeds included.
    #[cfg_attr(feature = "serde", serde(default))]
    pub portraits: Vec<super::portraits::CulturePortraits>,
    /// The original's own region base values (the campaign AI's `CAI_REGION_BASE_VALUE` beliefs,
    /// CONFIRMED layout, `analysis/ai/AI_RESEARCH.md` §4), by region, as the campaign source found
    /// them. A region without one gets the formula
    /// ([`super::region_value::stored_or_formula`]).
    #[cfg_attr(feature = "serde", serde(default))]
    pub region_base_values: BTreeMap<RegionId, i32>,
    /// The regions each human faction has received in accepted deals (faction `+0x938`, getter
    /// `0x008E66A0`, raised by `0x008E2B70` from the regions record's accept `0x00C18BF0`; saved
    /// in the `FACTION` record from version 0xB, reader `0x0087B7EE`, writer `0x00893B22`;
    /// CONFIRMED). It makes each later region demand of a human dearer
    /// ([`super::deal_value::human_demand_scaled`]). A faction without an entry has 0.
    /// PROVISIONAL: not yet read from an original save (the record's child index is not found),
    /// so a loaded game starts every faction at 0.
    #[cfg_attr(feature = "serde", serde(default))]
    pub deal_regions_received: BTreeMap<FactionId, u32>,
}

/// The campaign AI keys stored with a faction ([`World::ai_keys`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FactionAiKeys {
    /// `campaign_ai_managers` key (faction `+0x82C`).
    pub manager: String,
    /// `campaign_ai_personalities` key (faction `+0x838`).
    pub personality: String,
    /// The two further strings (`+0x844`, `+0x850`; `"default"` in every shipped faction, meaning
    /// UNKNOWN).
    pub extra: [String; 2],
}

/// Each human faction's spy-network sight discs: (position, radius) per character listed at its
/// last turn start ([`World::network_sight`]).
pub type NetworkSight = BTreeMap<FactionId, Vec<((f32, f32), f32)>>;

impl World {
    /// A new object id (characters, forces, units), unique among the ids of the loaded file and
    /// every id handed out before. Ids step by 8 like the original's pointer-like ids
    /// (SAVE_COMPAT.md §3; the step is our choice, PROVISIONAL). Ids at or above `0x7fff_0000`
    /// (negative ones included, as u32) are left out of the start, so new ids stay positive as
    /// i32. They are not reserved: the counter reaches them only after every id between the file's
    /// top id and them (about 2^28 for a vanilla file, whose ids are far lower).
    pub fn alloc_id(&mut self) -> u32 {
        if self.next_id == 0 {
            let top = self
                .characters
                .keys()
                .map(|c| c.0 as u32)
                .chain(self.forces.keys().map(|f| f.0))
                .chain(self.forces.values().flat_map(|f| f.units.iter().map(|u| u.id.0 as u32)))
                .chain(self.regions.keys().map(|r| r.0))
                .chain(self.regions.values().flat_map(|r| r.recruitment_queue.iter().map(|i| i.id.0 as u32)))
                .chain(self.factions.keys().map(|f| f.0 as u32))
                .filter(|&v| v < 0x7fff_0000)
                .max()
                .unwrap_or(0);
            self.next_id = (top / 8 + 1) * 8;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(8);
        id
    }

    /// The next unused force id: one more than the largest id in use (0 if there are none).
    /// PLACEHOLDER: how the original allocates new ids is UNKNOWN.
    pub fn next_force_id(&self) -> ForceId {
        self.forces
            .keys()
            .next_back()
            .map_or(ForceId(0), |id| ForceId(id.0.wrapping_add(1)))
    }

    /// The next unused unit id: one more than the largest unit id in any force (0 if none).
    /// PLACEHOLDER: how the original allocates new ids is UNKNOWN.
    pub fn next_unit_id(&self) -> UnitId {
        self.forces
            .values()
            .flat_map(|f| f.units.iter().map(|u| u.id))
            .max()
            .map_or(UnitId(0), |id| UnitId(id.0.wrapping_add(1)))
    }

    /// The faction's leader (the holder of its `faction_leader` post), if known.
    pub fn faction_leader(&self, faction: FactionId) -> Option<CharacterId> {
        self.faction_details.get(&faction)?.leader()
    }

    /// The faction whose governorship lists the region (`GOVERNORSHIP` #2), else the region's owner.
    /// The region's effect set takes its faction part from this faction (INFERRED from a vanilla save:
    /// `eur_bavaria`, owned by Austria but still in the dead Bavaria's governorship, shows Bavaria's
    /// government and none of Austria's minister effects). Normally it is the owner.
    pub fn governing_faction(&self, region: RegionId) -> Option<FactionId> {
        let owner = self.regions.get(&region)?.owner;
        let listed = |f: &FactionId| {
            self.faction_details.get(f).is_some_and(|d| d.posts.iter().any(|p| p.governorship.as_ref().is_some_and(|g| g.regions.contains(&region))))
        };
        if listed(&owner) {
            return Some(owner);
        }
        Some(self.faction_details.keys().find(|f| self.factions.contains_key(f) && listed(f)).copied().unwrap_or(owner))
    }

    /// [`Self::governing_faction`] of every region in one pass (for callers that need all of them).
    pub fn governing_factions(&self) -> BTreeMap<RegionId, FactionId> {
        // Regions listed in a living faction's governorship; the owner's own listing wins.
        let mut listed: BTreeMap<RegionId, Vec<FactionId>> = BTreeMap::new();
        for (f, d) in self.faction_details.iter().filter(|(f, _)| self.factions.contains_key(f)) {
            for g in d.posts.iter().filter_map(|p| p.governorship.as_ref()) {
                for r in &g.regions {
                    listed.entry(*r).or_default().push(*f);
                }
            }
        }
        self.regions
            .values()
            .map(|r| {
                let gov = match listed.get(&r.id) {
                    Some(l) if !l.contains(&r.owner) => l[0],
                    _ => r.owner,
                };
                (r.id, gov)
            })
            .collect()
    }

    /// The faction's capital region, if known.
    pub fn capital(&self, faction: FactionId) -> Option<RegionId> {
        self.faction_details.get(&faction)?.capital
    }

    /// True if the region is its owner's capital.
    pub fn is_capital(&self, region: RegionId) -> bool {
        self.regions.get(&region).is_some_and(|r| self.capital(r.owner) == Some(region))
    }

    /// Every faction in turn order: [`World::turn_order`] first, then any other faction in id
    /// order.
    pub fn factions_in_turn_order(&self) -> Vec<FactionId> {
        let mut out: Vec<FactionId> =
            self.turn_order.iter().copied().filter(|f| self.factions.contains_key(f)).collect();
        for id in self.factions.keys() {
            if !out.contains(id) {
                out.push(*id);
            }
        }
        out
    }

    /// The last record of `faction`'s economics history (record `(current − 1) mod 10`, `0x00BABE00`).
    pub fn last_economy_record(&self, faction: FactionId) -> Option<&EconomyRecord> {
        self.economy_history.get(&faction)?.last()
    }

    /// `faction`'s income of the last turn: the sum of categories 5..11 of its last economics record
    /// (`GetFactionLastTurnIncomeTotal` `0x00BBCC40` / `0x00BBE970`, CONFIRMED; the Wealth of
    /// [`CampaignModel::faction_rankings`]); 0 without a history. Integer sums wrap as the exe's.
    pub fn last_income(&self, faction: FactionId) -> i32 {
        self.last_economy_record(faction).map_or(0, |r| economy_sum(r, 5..12))
    }

    /// `faction`'s expenses of the last turn: categories 18..24 of its last record (`0x00BBE910`,
    /// CONFIRMED); 0 without a history.
    pub fn last_expenses(&self, faction: FactionId) -> i32 {
        self.last_economy_record(faction).map_or(0, |r| economy_sum(r, 18..25))
    }
}

/// One economics history record: the 25 integer categories of a turn (`ECONOMICS_DATA`, saved as
/// groups of 5 / 3 / 4 / 1 / 5 / 7).
pub type EconomyRecord = [i32; 25];

/// The original's economics history depth: a ring of 10 records (the index arithmetic `mod 10` of
/// `0x00BABE00` / `0x00A9B810` and the saver `0x00BD46E0`, CONFIRMED).
pub const ECONOMY_HISTORY_LEN: usize = 10;

/// The wrapping sum of categories `range` of a record (`CalculateIntRangeSum`).
pub fn economy_sum(r: &EconomyRecord, range: std::ops::Range<usize>) -> i32 {
    r[range].iter().fold(0i32, |s, v| s.wrapping_add(*v))
}

/// A faction. W3 §3.3 `FACTION` v18 (CONFIRMED structure; field meanings INFERRED).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Faction {
    /// Leading i32 object id (W3 §3.3).
    pub id: FactionId,
    /// Faction key, e.g. a `factions` table key (W3 §3.3).
    pub key: String,
    /// Treasury: the first scalar of `FACTION_ECONOMICS` (W3 §3.3, INFERRED meaning). An `i32`
    /// so it can go negative (the `PendingBankruptcy` event exists, W3 §6.4).
    pub treasury: i32,
    /// `GOVERNMENT` / `GOV_IMP` (W3 §3.3).
    pub government: GovernmentType,
    /// `GOVERNMENT` #1 string, the `government_types` key (e.g. `gov_empire` for france,
    /// CONFIRMED in the eur startpos); used to look up `government_types_to_effects`. Empty for
    /// the rebel faction.
    pub government_key: String,
    /// Tax level of the lower classes (a `taxes_levels` key), loaded from the faction's governorship
    /// (`GOVERNORSHIP_TAXES`, one governorship per faction in every shipped campaign);
    /// [`DEFAULT_TAX_LEVEL`](super::rules::DEFAULT_TAX_LEVEL) when the file has none.
    pub tax_lower: String,
    /// Tax level of the upper classes (a `taxes_levels` key). Same caveat.
    pub tax_upper: String,
    /// Stance towards each other faction: `DIPLOMACY_MANAGER/DIPLOMACY_RELATIONSHIPS_ARRAY`
    /// (W3 §3.4, CONFIRMED structure). A missing entry means [`Stance::Neutral`]. Always change it
    /// through [`World::set_stance`](crate::campaign::World::set_stance) so both sides agree.
    pub diplomacy: BTreeMap<FactionId, Stance>,
}

/// A building standing in a region slot. W3 §3.7: each slot has an optional `BUILDING`
/// {u32 health (100), building level key, faction, government} (CONFIRMED structure).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuildingRef {
    /// Building level key (FK to `building_levels`, DB_CAMPAIGN_TABLES §4).
    pub level_key: String,
    /// Health; 100 in the startpos (W3 §3.7).
    pub health: u32,
}

impl BuildingRef {
    /// Damaged: health below 100 (CONFIRMED: the exe tests `health > 99`, e.g. `0x00A62380` for the effects and
    /// `0x00B43CA0` for recruitable flag 8). A damaged building gives no effects, flags its units, is not
    /// upgraded, and is what a repair restores.
    pub fn is_damaged(&self) -> bool {
        self.health < 100
    }
}

/// One `REGION_SLOT` of a region (W3 §3.4, CONFIRMED structure).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegionSlot {
    /// `REGION_SLOT` #3 key, e.g. `settlement:eur_france:paris:settlement_4_slot:0` or
    /// `timber:eur_france:limoges` (CONFIRMED strings).
    pub key: String,
    /// Slot type (a `building_chain_to_slots` slot type): the 4th part of a `settlement:` key,
    /// else the `campaign_map_slots` / `campaign_map_towns_and_ports` type of the key.
    pub slot_type: String,
    /// The building standing in the slot.
    pub building: Option<BuildingRef>,
    /// `REGION_SLOT` #6/#7: the slot's map position (20-bit fixed point; CONFIRMED: a town
    /// slot's position equals the position of the general garrisoned there). `None` if unknown.
    pub position: Option<(Fixed20, Fixed20)>,
    /// The slot has its own recruitment queue (`REGION_RECRUITMENT_MANAGER`): a port.
    pub port: bool,
    /// The faction holding the slot: its `GARRISON_RESIDENCE` #0 (a faction id, CONFIRMED against the
    /// `FACTION` ids of the vanilla saves). `None` = the region owner. A slot held by another faction
    /// (an occupied town, or settlement slots still held by the previous owner) adds no building effects.
    pub holder: Option<FactionId>,
    /// `REGION_SLOT` #2 u32: the slot's id (= its residence's #1; a school's id is what a technology's
    /// `techs[]` #3 names as its researcher, CONFIRMED in the vanilla saves). 0 when unknown.
    pub id: u32,
}

/// A building being built. Saves hold `BUILDING_CONSTRUCTION_ITEM` {u32, bool, u32 turns done
/// (INFERRED), u32 total turns (INFERRED), u32 cost (INFERRED), utf16 level key (CONFIRMED)}.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConstructionItem {
    /// The slot being built in.
    pub slot: SlotRef,
    /// The building level being built.
    pub level_key: String,
    /// End-of-turns left until it is finished.
    pub turns_remaining: u32,
    /// The cost stored with the item (`BUILDING_CONSTRUCTION_ITEM` #4): the construction or repair cost.
    pub cost: i32,
}

/// A settlement. W3 §3.7 `SETTLEMENT` v2/v3 (CONFIRMED structure): position in
/// `SIEGEABLE_GARRISON_RESIDENCE` (i32 fixed point), key `"settlement:<region>:<town>"`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Settlement {
    /// Settlement key, e.g. `"settlement:<region>:<town>"`.
    pub key: String,
    /// Map position, 20-bit fixed point (W3 §2.4, CONFIRMED).
    pub position: (Fixed20, Fixed20),
}

/// A unit being recruited. Saves hold `RECRUITMENT_ITEM` v2 {i32 id, i32, i32, #3 u32 turns
/// remaining (INFERRED), #4 u32 cost (INFERRED), bool, #6 unit key (CONFIRMED), ...}.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RecruitmentItem {
    /// The item's id (see [`RecruitmentItemId`]): what `CancelRecruitment` names.
    pub id: RecruitmentItemId,
    /// Unit key (FK to the `units` table).
    pub unit_key: String,
    /// End-of-turns left until the unit appears.
    pub turns_remaining: u32,
    /// What was paid (refunded on cancel).
    pub cost: i32,
    /// The commander the unit was recruited through (item +0x18, `RECRUITMENT_ITEM` #2; CONFIRMED:
    /// `QueueRecruitmentItemForUnit` `0x00B58DD0` stores the `CCQ` command's target there, and a
    /// commander's recruitment panel lists the items whose +0x18 is he, `0x00B26020`). `None` for a
    /// settlement's own recruit.
    #[cfg_attr(feature = "serde", serde(default))]
    pub target: Option<CharacterId>,
}

/// The record a loaded recruitment item came from ([`World::recruitment_sources`]): its region's
/// recruitment manager and its index in that manager's `REGION_RECRUITMENT_ITEM_ARRAY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RecruitmentSource {
    /// `None`: the region's own `REGION_RECRUITMENT_MANAGER` (land); `Some(i)`: the manager of
    /// `REGION_SLOT_ARRAY` item `i` (a port, naval).
    pub port_slot: Option<usize>,
    /// The item's index in the manager's array (every array item counts).
    pub index: usize,
}

/// A region. W3 §3.7 `REGION` v5 (CONFIRMED structure).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Region {
    /// Region id.
    pub id: RegionId,
    /// Region key (e.g. `eur_<name>`).
    pub key: String,
    /// Owning faction (`REGION_OWNERSHIPS`, W3 §3.3).
    pub owner: FactionId,
    /// The region's settlement.
    pub settlement: Settlement,
    /// `REGION_SLOT_MANAGER/REGION_SLOT_ARRAY` slots in file order (W3 §3.7).
    pub slots: Vec<RegionSlot>,
    /// The building in `REGION_SLOT_MANAGER/ROAD_SLOT` (an `sRoads*` level or none). Its level
    /// sets the road level of the region's roads (see `CampaignModel::road_level`).
    pub road: Option<BuildingRef>,
    /// The building in `REGION_SLOT_MANAGER/FORTIFICATION_SLOT`: the settlement's walls / fort (a `building_levels` key;
    /// the map draws `<city template>_fortifications_lvl<level + 1>`). `None` without walls. Its effects are not
    /// counted ([`Region::effect_buildings`]: the exe's effect walk `0x00A62380` reads the slot array; INFERRED). It is
    /// built, upgraded and repaired through [`SlotRef::Walls`] (slot type `settlement_fortification`); a
    /// capture rolls damage on it
    /// ([`capture`](super::capture)).
    pub fortification: Option<BuildingRef>,
    /// The live population (`POPULATION/REGION_FACTORS` #2, the population object's +0x54 = region +0x7C;
    /// CONFIRMED: the reader `0x00A4C340`; growth, conversion, garrison repression and the panel read it).
    /// `POPULATION` #1 differs from it once the population has grown (`orig_fr_may1811`) and is not read.
    pub population: u32,
    /// The rest of the population state (`REGION_FACTORS`, see [`super::population`]).
    #[cfg_attr(feature = "serde", serde(default))]
    pub population_state: super::population::PopulationState,
    /// `REGION` #9 u32: the region's base GDP (region +0xB8, CONFIRMED): the start of the GDP sum
    /// (see [`economy::recompute_region`](super::economy::recompute_region)).
    pub base_gdp: u32,
    /// `REGION` #10 u32: the region's GDP (region +0xBC in the exe). CONFIRMED: the REGION writer
    /// (0x00A51E30) writes +0xBC as child #10; the tax functions (0x00AB5560 upper, 0x00A8C2E0
    /// lower) tax `gdp + town_wealth`, and the faction GDP (0x008C3110) is
    /// `faction_gdp_other + Σ gdp`. The original recomputes it each turn from the region's slots
    /// (`slots_gdp_values`, building effects; 0x00A6AFC0): `economy::recompute_region` at each round
    /// end, which reproduces every region of the shipped start positions.
    pub gdp: u32,
    /// `REGION` #12 u32: town wealth (region +0xCC). CONFIRMED: written as #12, part of the tax
    /// base, and updated each turn as `max(0, town_wealth + town_wealth_growth)` (0x00AB4410).
    pub town_wealth: u32,
    /// `REGION` #15 i32: town wealth growth per turn (region +0xD8, CONFIRMED position and use).
    /// The original recomputes it each turn (building `tw_growth*` effects, taxes, unrest;
    /// 0x00A6AFC0): `economy::recompute_region` at each round end.
    pub town_wealth_growth: i32,
    /// `REGION` #17 (region +0xEC, an integer; UNKNOWN meaning, 0 in every shipped file): subtracted
    /// from the tax modifier of the town wealth growth (CONFIRMED use, 0x00A6AFC0).
    pub wealth_growth_offset: i32,
    /// `REGION` #18 i32 (region +0xF0): `-town_wealth_growth_discontent_reduction` while the region
    /// has unrest, else 0 (set by 0x00AB42F0; the unrest condition is not modelled, the value is kept
    /// as loaded). Part of the town wealth growth.
    pub discontent_growth: i32,
    /// `REGION` #19 bool: the region is exempt from taxes (region +0xE4; CONFIRMED: the effective
    /// tax rate 0x00BA4210 is 0 when it is set).
    pub tax_exempt: bool,
    /// `POPULATION/REGION_FACTORS/RELIGION_BREAKDOWN`: (religion key, share 0..1). Data the religion
    /// factor of public order reads.
    pub religions: Vec<(String, f32)>,
    /// Per population class (`upper` / `lower` / `middle`): the stored base of the education factor
    /// (class +0x70, `POPULATION_CLASS` #11) and of the war-results factor (class +0x74, #12).
    pub class_bases: Vec<(String, i32, i32)>,
    /// Units being recruited here, in the order they were issued (W3 §4).
    pub recruitment_queue: Vec<RecruitmentItem>,
    /// Buildings being built here, in the order they were issued.
    pub construction: Vec<ConstructionItem>,
    /// The army garrisoned in the settlement: `SIEGEABLE_GARRISON_RESIDENCE` #12 of the settlement,
    /// with that `ARMY` #5 = the residence (CONFIRMED, SAVE_COMPAT.md §4); one army at most.
    /// Recruited land units join it, up to the units per army (`CampaignModel::max_units`).
    pub garrison: Option<ForceId>,
    /// The navy that last received ships recruited in the region's port, while it is still there
    /// (our own bookkeeping: navies have no residence link in the original).
    pub fleet: Option<ForceId>,
}

/// A region's building slot, as the commands, construction items and the HUD address it: the
/// region's `REGION_SLOT_MANAGER` holds a `REGION_SLOT_ARRAY` ([`Region::slots`]), a `ROAD_SLOT`
/// ([`Region::road`]) and a `FORTIFICATION_SLOT` (the settlement walls, [`Region::fortification`]);
/// all three are ordinary building slots in the exe (same slot class, CONFIRMED by the loader
/// `0x00A4DCB0`). Ordered as the exe walks a region's slots (`0x009B5AF0`: the slot list, then
/// the walls, then the road; CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SlotRef {
    /// A slot of [`Region::slots`], by index.
    Slot(usize),
    /// The settlement walls' slot (slot type `settlement_fortification`).
    Walls,
    /// The road slot.
    Road,
}

/// A fort standing in a region: one item of a `REGION`'s `FORT_ARRAY`.
///
/// This is **not** the settlement's walls ([`Region::fortification`], a `building_levels` key in the
/// `FORTIFICATION_SLOT`): a fort is a separate map object a general builds and armies move into.
/// What the shipped loc says it is (CONFIRMED, read from the install's localisation):
/// - `army_fort_Tooltip_12006e` = "Build fort || Building a fort requires a general and you must be
///   within your own region. || ...";
/// - `campaign_map_tooltips_advice_line_armynon_player_fort` = "Forts allow armies to remain
///   protected while outside of their cities. If the fort is already occupie[d] ...";
/// - `campaign_map_tooltips_tooltip_line_armyplayer_fort` = "Right click to enter fort", and
///   `..._rakeplayer_fort` the same, `..._army_can_ambushnon_player_fort` = "Right click to take
///   fort".
///
/// So a fort belongs to a region, garrisons an army, and can be entered or taken -- a garrison
/// residence of its own, which is why the exe's fort object registers itself in the campaign model
/// next to the region's (`FUN_00AEB190`, +0x104 of the model, INFERRED: no decompile kept).
///
/// **The record layout is only partly known.** The region reader `0x00A51E30` walks `FORT_ARRAY`
/// and, per item, reads **one `u32`** of its own and then lets the fort object read its fields
/// (`FUN_00AFD010`, INFERRED: no decompile kept); the fort object's loader `FUN_00AEB190` reads a map position
/// (two f32, stored at fort +0x120) and then a string (fort +0x188), plus a second string when the
/// record says it is version 2 and a third when it says version 4. The meaning of that leading
/// `u32`, of the strings and of the version number is **UNKNOWN** -- no shipped file has an item to
/// read, so nothing can confirm them. What the game does with a fort (its garrison, its strength,
/// whether armies inside it can be attacked) is also UNKNOWN here.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Fort {
    /// The item's leading `u32` ([`FortId`]). The exe's id lookup turns it into the fort's object;
    /// whether it is the fort's own id is UNKNOWN.
    pub id: FortId,
    /// The region whose `FORT_ARRAY` held this fort.
    pub region: RegionId,
    /// The fort's map position, if the record gave one (the exe's two f32, CONFIRMED that a
    /// position is stored). `None` when it is missing.
    pub position: Option<(Fixed20, Fixed20)>,
    /// The fort's first stored string (fort +0x188). Its meaning and the original's naming rule are
    /// UNKNOWN; `selection_name` shows the region's settlement for a fort instead.
    pub key: String,
}

impl Region {
    /// What the region's constructions change when they finish, as the predictions count it: per
    /// construction item, its slot, the level being built and the building it replaces there
    /// (`0x00A67490` / `0x00A798F0` for the predicted effect set, `0x00A6AFC0` with its predicted flag
    /// for each slot).
    pub fn construction_changes(&self) -> impl Iterator<Item = (SlotRef, &str, Option<&BuildingRef>)> {
        self.construction.iter().map(|c| (c.slot, c.level_key.as_str(), self.building_at(c.slot)))
    }

    /// The building standing in a slot.
    pub fn building_at(&self, slot: SlotRef) -> Option<&BuildingRef> {
        self.construction_slot(slot).and_then(|(_, b)| b)
    }

    /// A slot's slot type and standing building, or `None` when the index names no slot. The
    /// walls and the road are ordinary building slots in the exe (`settlement_fortification` /
    /// `settlement_road`, CONFIRMED by `0x00A8B580` / `0x00A8B970`).
    pub fn construction_slot(&self, slot: SlotRef) -> Option<(&str, Option<&BuildingRef>)> {
        Some(match slot {
            SlotRef::Walls => ("settlement_fortification", self.fortification.as_ref()),
            SlotRef::Slot(i) => {
                let s = self.slots.get(i)?;
                (s.slot_type.as_str(), s.building.as_ref())
            }
            SlotRef::Road => ("settlement_road", self.road.as_ref()),
        })
    }

    /// The building slot `slot` names, to change what stands there (`None` when the index names no
    /// slot).
    pub fn building_mut(&mut self, slot: SlotRef) -> Option<&mut Option<BuildingRef>> {
        match slot {
            SlotRef::Walls => Some(&mut self.fortification),
            SlotRef::Slot(i) => self.slots.get_mut(i).map(|s| &mut s.building),
            SlotRef::Road => Some(&mut self.road),
        }
    }

    /// Is `slot` held by the region's owner? A town / port slot can be held by another faction
    /// (`RegionSlot::holder`); the walls and the road always are the owner's.
    pub fn slot_held(&self, slot: SlotRef) -> bool {
        match slot {
            SlotRef::Slot(i) => self.slots.get(i).is_some_and(|s| !self.slot_occupied(s)),
            SlotRef::Walls | SlotRef::Road => true,
        }
    }

    /// Is the slot `s` (one of this region's) held by another faction than the region's owner (`0x00A91FC0`,
    /// CONFIRMED)? Such a slot's building gives no effects and flags its units (recruitable flag 0x10).
    pub fn slot_occupied(&self, s: &RegionSlot) -> bool {
        s.holder.is_some_and(|h| h != self.owner)
    }

    /// Every building standing in the region (slots and road), in slot order.
    pub fn buildings(&self) -> impl Iterator<Item = &BuildingRef> {
        self.slots.iter().filter_map(|s| s.building.as_ref()).chain(self.road.as_ref())
    }

    /// The buildings whose effects count (`0x00A62380`, CONFIRMED): at full health (> 99) in a slot the
    /// region owner holds (`0x00A91FC0`: the slot's faction = the region's owner). The road slot has no
    /// holder of its own here and always counts (INFERRED).
    pub fn effect_buildings(&self) -> impl Iterator<Item = &BuildingRef> {
        self.slots
            .iter()
            .filter(|s| !self.slot_occupied(s))
            .filter_map(|s| s.building.as_ref())
            .chain(self.road.as_ref())
            .filter(|b| !b.is_damaged())
    }
}

/// A character. W3 §3.5 `CHARACTER` v12/v14 (CONFIRMED structure).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Character {
    /// i32 id.
    pub id: CharacterId,
    /// Owning faction (the faction whose `CHARACTER_ARRAY` holds it).
    pub faction: FactionId,
    /// utf16 type.
    pub kind: CharacterKind,
    /// `LOCOMOTABLE` x, z in 20-bit fixed point (W3 §3.5, CONFIRMED encoding).
    pub position: (Fixed20, Fixed20),
    /// `LOCOMOTABLE` #9: action points left this turn. INFERRED from the user's saves, where the
    /// value ranges from 0 to above the type's base (General 0..44 with base 26) and is lower for
    /// characters that have moved.
    pub movement_points: i32,
    /// This turn's maximum action points: [`Character::base_movement_points`] times the force factor
    /// for a force's commander ([`force_action_point_factor`](super::turn::force_action_point_factor)),
    /// set at each refill. CONFIRMED semantics: the original replaces the movement maximum with the
    /// scaled value (0x008ABE40: unit +0x6C = round(+0x6C × factor), the unscaled value kept at
    /// +0x114), so the bonus is part of the maximum the UI shows, not added on top of it.
    pub max_movement_points: i32,
    /// `LOCOMOTABLE` #8: the type's base action points. CONFIRMED equal to `agents` #1 for every
    /// character in the eur startpos and the saves (General 26, colonel 25, admiral 90, ...): the
    /// files store the unscaled base.
    pub base_movement_points: i32,
    /// The region whose settlement the character is inside, if any (our own field; the
    /// original keeps garrisons in `SIEGEABLE_GARRISON_RESIDENCE`, not decoded yet).
    pub garrisoned_in: Option<RegionId>,
}

/// Government type. W3 §3.3 (CONFIRMED): the `GOVERNMENT` block holds a `GOV_IMP` record named
/// `GOVERNMENT::ABSOLUTE_MONARCHY` / `CONSTITUTIONAL_MONARCHY` / `REPUBLIC`.
/// (The DB `government_types` table has 4 rows, DB_CAMPAIGN_TABLES §6; the 4th is not seen in the
/// startpos and is not modelled.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GovernmentType {
    /// `GOVERNMENT::ABSOLUTE_MONARCHY` / DB key `gov_absolute_monarchy`.
    AbsoluteMonarchy,
    /// `GOVERNMENT::CONSTITUTIONAL_MONARCHY` (britain in eur).
    ConstitutionalMonarchy,
    /// `GOVERNMENT::REPUBLIC` / DB key `gov_republic` (netherlands, ita_venice in eur).
    Republic,
}

impl GovernmentType {
    /// The `GOV_IMP` record name used in the ESF (CONFIRMED strings, W3 §3.3).
    pub fn esf_name(self) -> &'static str {
        match self {
            GovernmentType::AbsoluteMonarchy => "GOVERNMENT::ABSOLUTE_MONARCHY",
            GovernmentType::ConstitutionalMonarchy => "GOVERNMENT::CONSTITUTIONAL_MONARCHY",
            GovernmentType::Republic => "GOVERNMENT::REPUBLIC",
        }
    }

    /// The DB key of a government type: `government_types` #0, the ESF `GOVERNMENT` #1 string, with the
    /// same words as [`Self::esf_name`] (CONFIRMED in the vanilla start positions, where both strings are
    /// readable). `government_types` has a 4th row, `gov_empire` (france's key, CAMPAIGN_PLAY §…), which
    /// the model has no `GOV_IMP` record for and no variant for.
    pub fn db_key(self) -> &'static str {
        match self {
            GovernmentType::AbsoluteMonarchy => "gov_absolute_monarchy",
            GovernmentType::ConstitutionalMonarchy => "gov_constitutional_monarchy",
            GovernmentType::Republic => "gov_republic",
        }
    }

    /// The government type a DB key names, or `None` when the model does not have it (`gov_empire`;
    /// the loader keeps a PLACEHOLDER for the keys it cannot name, [`Self::from_esf_name`]).
    pub fn from_db_key(key: &str) -> Option<Self> {
        [
            GovernmentType::AbsoluteMonarchy,
            GovernmentType::ConstitutionalMonarchy,
            GovernmentType::Republic,
        ]
        .into_iter()
        .find(|g| g.db_key() == key)
    }

    /// Parses an ESF `GOV_IMP` record name.
    pub fn from_esf_name(name: &str) -> Option<Self> {
        [
            GovernmentType::AbsoluteMonarchy,
            GovernmentType::ConstitutionalMonarchy,
            GovernmentType::Republic,
        ]
        .into_iter()
        .find(|g| g.esf_name() == name)
    }
}

/// A diplomatic stance. W3 §3.4 (CONFIRMED strings in `DIPLOMACY_RELATIONSHIP` v14) and
/// DB_CAMPAIGN_TABLES §9 `stances` (5 rows, matching).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Stance {
    /// `"neutral"`. Also our default when no relationship is stored.
    #[default]
    Neutral,
    /// `"war"`.
    War,
    /// `"allied"`.
    Allied,
    /// `"protectorate"`.
    Protectorate,
    /// `"patron"`.
    Patron,
}

impl Stance {
    /// The stance string stored in the ESF (CONFIRMED, W3 §3.4).
    pub fn esf_name(self) -> &'static str {
        match self {
            Stance::Neutral => "neutral",
            Stance::War => "war",
            Stance::Allied => "allied",
            Stance::Protectorate => "protectorate",
            Stance::Patron => "patron",
        }
    }

    /// Parses an ESF stance string.
    pub fn from_esf_name(name: &str) -> Option<Self> {
        [
            Stance::Neutral,
            Stance::War,
            Stance::Allied,
            Stance::Protectorate,
            Stance::Patron,
        ]
        .into_iter()
        .find(|s| s.esf_name() == name)
    }

    /// The stance the *other* faction holds back. Neutral, war and alliance are mutual, while
    /// protectorate and patron are the two ends of one relationship.
    /// INFERRED: which side stores "protectorate" and which "patron" is not confirmed.
    pub fn mirror(self) -> Self {
        match self {
            Stance::Protectorate => Stance::Patron,
            Stance::Patron => Stance::Protectorate,
            other => other,
        }
    }
}

/// Character types seen in the startpos `CHARACTER` utf16 type field (W3 §3.5, CONFIRMED
/// strings: General, colonel, admiral, captain, minister, gentleman, rake), plus six agent kinds
/// found in the other campaigns' start positions (`Assassin` .. `Guerilla`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CharacterKind {
    /// `"General"` (capital G in the ESF).
    General,
    /// `"colonel"`.
    Colonel,
    /// `"admiral"`.
    Admiral,
    /// `"captain"`.
    Captain,
    /// `"minister"` (off-map, at (0, 0)).
    Minister,
    /// `"gentleman"`.
    Gentleman,
    /// `"rake"`.
    Rake,
    // The six kinds below were found by `ntw_campaign` in the egy / mp_egy / mp_eur / spa start
    // positions (CONFIRMED strings, spelling and case exactly as stored). They are appended at the
    // end so the numbers `state_hash` uses for the older kinds stay the same.
    /// `"assassin"` (egy, mp_egy, mp_eur).
    Assassin,
    /// `"Eastern_Scholar"` (capitals as stored; mp_egy, mp_eur).
    EasternScholar,
    /// `"catholic_missionary"` (spa).
    CatholicMissionary,
    /// `"orthodox_missionary"` (spa).
    OrthodoxMissionary,
    /// `"Protestant_Missionary"` (capitals as stored; spa).
    ProtestantMissionary,
    /// `"guerilla"` (spa).
    Guerilla,
}

impl CharacterKind {
    /// Every kind, in declaration order.
    pub const ALL: [CharacterKind; 13] = [
        CharacterKind::General,
        CharacterKind::Colonel,
        CharacterKind::Admiral,
        CharacterKind::Captain,
        CharacterKind::Minister,
        CharacterKind::Gentleman,
        CharacterKind::Rake,
        CharacterKind::Assassin,
        CharacterKind::EasternScholar,
        CharacterKind::CatholicMissionary,
        CharacterKind::OrthodoxMissionary,
        CharacterKind::ProtestantMissionary,
        CharacterKind::Guerilla,
    ];

    /// The type string stored in the ESF (CONFIRMED spelling, W3 §3.5).
    pub fn esf_name(self) -> &'static str {
        match self {
            CharacterKind::General => "General",
            CharacterKind::Colonel => "colonel",
            CharacterKind::Admiral => "admiral",
            CharacterKind::Captain => "captain",
            CharacterKind::Minister => "minister",
            CharacterKind::Gentleman => "gentleman",
            CharacterKind::Rake => "rake",
            CharacterKind::Assassin => "assassin",
            CharacterKind::EasternScholar => "Eastern_Scholar",
            CharacterKind::CatholicMissionary => "catholic_missionary",
            CharacterKind::OrthodoxMissionary => "orthodox_missionary",
            CharacterKind::ProtestantMissionary => "Protestant_Missionary",
            CharacterKind::Guerilla => "guerilla",
        }
    }

    /// Parses an ESF type string.
    pub fn from_esf_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.esf_name() == name)
    }
}

/// A unit on the campaign map. W3 §3.6 `UNIT` v3 (CONFIRMED structure: unit record key,
/// i32 unit_id, u32 men, u32 max_men, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CampaignUnit {
    /// i32 unit id.
    pub id: UnitId,
    /// `UNIT_RECORD_KEY` (FK to the `units` table).
    pub unit_key: String,
    /// Current number of men.
    pub men: u32,
    /// Full-strength number of men.
    pub max_men: u32,
    /// `UNIT` #10: the character attached to the unit (the general for his bodyguard unit, the
    /// colonel or captain of a unit raised alone; that character's `CHARACTER` #5 is the unit).
    /// CONFIRMED in the saves (SAVE_COMPAT.md §4).
    pub character: Option<CharacterId>,
    /// The unit officer's name, (forename, surname) localisation keys (`UNIT` `COMMANDER_DETAILS`
    /// #0 / #1, CONFIRMED form): a colonel or general who takes the unit over carries it
    /// ([`super::names`]). Empty when unknown.
    #[cfg_attr(feature = "serde", serde(default))]
    pub officer_name: (String, String),
}

/// The full-strength size of a unit of `unit_key`: the value a freshly raised unit's `men` and
/// `max_men` both take (BACKLOG 0-B open item 7, the `num_men` sub-item that was PROVISIONAL).
///
/// **CONFIRMED from the units the original itself stored**, not from the exe: every unit in all nine
/// vanilla saves carries `max_men` equal to
/// - `unit_stats_land.num_men` for a land unit (`UnitRules::men`), or
/// - the **sum of the `unit_stats_naval` crew triple** (c17/c18/c19, [`super::rules::CampaignRules`]
///   `ships[..].crews`) for a ship — `3_Decker_British_1st_Rate` 50+50+204 = 304,
///   `Trade_Ship_Indiaman` 0+28+112 = 140, `Small_Ottoman_Galley` 10+14+4 = 28, and so on for every
///   class in the list,
///
/// with **no exception in 3174 units** over those nine files
/// (`recruited_unit_size_is_num_men_for_land_and_the_crew_sum_for_ships`,
/// `crates/ntw_campaign/tests/economy_fidelity.rs`). `men` equals `max_men` on every unit except the
/// ones carrying battle casualties (9 of 460 in `auto_nr4_t4`, 5 of 484 in `orig_over_nr4_0252`, none
/// in the four spa saves), which is the expected damage reading — so a new unit starts full.
///
/// A key with neither a land size nor a crew gives 1 (no such key in the shipped data).
pub fn recruited_unit_size(rules: &super::rules::CampaignRules, unit_key: &str) -> u32 {
    if let Some(u) = rules.units.get(unit_key)
        && u.men > 0
    {
        return u.men;
    }
    rules
        .ships
        .get(unit_key)
        .map(|s| s.crews.iter().map(|c| (*c).max(0) as u32).sum::<u32>())
        .filter(|n| *n > 0)
        .unwrap_or(1)
}

/// An army or navy. W3 §3.6 `ARMY` v2 / `NAVY` v1 → `MILITARY_FORCE`
/// {u32 force_id, u32 commander_character_id, u32[]} + `UNITS_ARRAY` (CONFIRMED structure).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MilitaryForce {
    /// u32 force id.
    pub id: ForceId,
    /// Owning faction (the faction whose `ARMY_ARRAY` holds it).
    pub faction: FactionId,
    /// Commanding character, if any. The force has no position of its own here: it moves with
    /// its commander (INFERRED from `LOCOMOTABLE` being on the character).
    pub commander: Option<CharacterId>,
    /// `UNITS_ARRAY`.
    pub units: Vec<CampaignUnit>,
    /// `true` for a `NAVY`, `false` for an `ARMY`.
    pub is_navy: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esf_names_round_trip() {
        for g in [
            GovernmentType::AbsoluteMonarchy,
            GovernmentType::ConstitutionalMonarchy,
            GovernmentType::Republic,
        ] {
            assert_eq!(GovernmentType::from_esf_name(g.esf_name()), Some(g));
        }
        for s in ["neutral", "war", "allied", "protectorate", "patron"] {
            assert_eq!(Stance::from_esf_name(s).unwrap().esf_name(), s);
        }
        for k in [
            "General",
            "colonel",
            "admiral",
            "captain",
            "minister",
            "gentleman",
            "rake",
            "assassin",
            "Eastern_Scholar",
            "catholic_missionary",
            "orthodox_missionary",
            "Protestant_Missionary",
            "guerilla",
        ] {
            assert_eq!(CharacterKind::from_esf_name(k).unwrap().esf_name(), k);
        }
        assert_eq!(Stance::from_esf_name("enemy"), None);
    }

    #[test]
    fn stance_mirror() {
        assert_eq!(Stance::Protectorate.mirror(), Stance::Patron);
        assert_eq!(Stance::Patron.mirror(), Stance::Protectorate);
        assert_eq!(Stance::War.mirror(), Stance::War);
    }

    #[test]
    fn next_ids() {
        let mut w = World::default();
        assert_eq!(w.next_force_id(), ForceId(0));
        assert_eq!(w.next_unit_id(), UnitId(0));
        w.forces.insert(
            ForceId(4),
            MilitaryForce {
                id: ForceId(4),
                faction: FactionId(1),
                commander: None,
                units: vec![CampaignUnit {
                    id: UnitId(9),
                    unit_key: "test_unit".into(),
                    men: 1,
                    max_men: 1,
                    character: None,
                    officer_name: Default::default(),
                }],
                is_navy: false,
            },
        );
        assert_eq!(w.next_force_id(), ForceId(5));
        assert_eq!(w.next_unit_id(), UnitId(10));
    }
}
