//! Commerce raids (CAMPAIGN_FIDELITY.md "Commerce raids"): an army or a navy ordered to raid trade
//! (`CMO_RAID_TRADE_ROUTE`, the order's vtable slot `0x01361694` = `0x0094BA50`, CONFIRMED) stands
//! next to the trade segments it raids and takes a share of every route along them that belongs to
//! its raid's target.
//!
//! - **Starting a raid** (`0x009D3AC0` → `0x00BA7680` → `0x00BA7600`, CONFIRMED): every trade segment
//!   within [`RAID_RADIUS`] of his position ([`segment_distance`]) that is raidable for his faction
//!   ([`CampaignModel::segment_raidable`], `0x00BA7DF0`), taken in turn, gets one raid entry with its
//!   target ([`CampaignModel::raid_target`], `0x00BAFB20`), and each entry runs the price update with
//!   flag 0 (`AddCommerceRaidEntry` `0x00BA76A0`). With none in range the order is cancelled
//!   (`0x00921CD0`). His earlier raids are ended by the order's start, not here.
//! - **Moving** (the move order's start slot `0x0095D940`, vtable `0x0135F498`, CONFIRMED: once the
//!   path is accepted, `0x0095DA8C`; the step update `0x00A285E0` also ends them on every step it runs
//!   with an argument other than 1, `0x00A28605`; that every model move is such an order is INFERRED):
//!   a raiding character (raid list +0x2D0 not empty, `IsListEmpty` `0x008BC550`) loses all his raids
//!   ([`CampaignModel::end_character_raids`], `EndCharacterCommerceRaids` `0x009DA140`).
//! - **A change of commander** hands the raids on (`CampaignModel::hand_over_raids`, CONFIRMED): when
//!   the old commander raided, his raids end and the new one raids where he stands, the old one's place
//!   (`0x009D3AC0`). The force's observer `0x008DB3E0` does it when its commander dies or leaves (the
//!   character destructor `0x0099D2D0` notifies it). Three other commander changes hand the raids on
//!   too ([`CampaignModel::hand_over_raids_on_command_change`], CONFIRMED): each notes the force's
//!   commander, his position and whether he raids before the change, and when he raided and the
//!   force's commander is now someone else, ends his raids and has the new commander raid at the
//!   noted position. A General hired into an army (`0x00A164C0`, `hire_general`); the field promotion
//!   (`0x008E1C20` land, `0x008E2260` naval, `promote_unit`); an admiral hired into a fleet
//!   (`0x00A16110`, `hire_admiral`): the ship's character he replaces is destroyed either way
//!   (`0x00A0C6F0`); when that one was the raiding commander, the destruction ends his raids (flag set
//!   at `0x00A161CB`) and the new commander still raids at the noted position, the same outcome. The
//!   rest (the force split `0x008B9030`, which hands the raids on only when the remaining force's new
//!   commander is a new officer, `0x008D47B0`, `0x008D5F10`) are paths our model does not have yet.
//! - **Ending**: his death (`0x0099D62E`); a peace (`0x00B262C0` → `0x00A25570`: every commander of either
//!   side with a raid on the other side loses all his raids); the raid check of every price update
//!   (`0x00BD1020` → `0x00BA4A70` → `0x008B3690`: a raider no longer raidable on one of his segments loses
//!   all his raids). Every end runs the price update with flag 0 (`0x009DA140` → `0x00BCB020(0)`).
//! - **The take** (`0x00B83C30` for an international route, `0x00B83360` for a domestic one, the same
//!   pass): over the route's legs, the characters raiding its segments whose target is the route's
//!   owner, once each; each one's share ([`CampaignModel::force_raid_share`]) summed per faction; when
//!   the shares sum above 1 each is divided by the sum; each faction takes
//!   [`raid_take`] of the route (seam [`trade.raid_take`](super::seams::RAID_TAKE)). The raider's
//!   faction books the take as income category 9 (`0x00BBE9A0` group 1,
//!   `finances_commerce_raiding_income`) as the last price update left it
//!   ([`super::economy::book_raid_income`]); the international route's owner earns its net value
//!   (`GetTradeRouteNetValue` `0x00B79B20`: total less the takes), and a domestic route's owner its
//!   volume less the raided part (`0x00B1D1C0` / `0x00B1D1F0`, [`CampaignModel::domestic_net_volumes`]).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use super::effects::{BonusKind, Effects};
use super::ids::{CharacterId, FactionId, ForceId};
use super::trade::{TradeLeg, TradePath};
use super::world::CampaignModel;
use super::Stance;
use crate::fixed::Fixed20;

/// How close a character has to stand to a trade segment to raid it (map units): the distance test
/// `0x00BCA060` keeps a segment nearer than 0.5 (CONFIRMED, reached from `0x009D3AC0` through
/// `0x00BCA0F0` → `0x00BCA140`).
pub const RAID_RADIUS: f32 = 0.5;

/// The share one movement point of a unit gives (`0x008B6C20`: the constant at `0x01325C88`,
/// `0x3D4CCCCD`, CONFIRMED).
pub const RAID_SHARE_PER_MOVE: f32 = 0.05;

/// The chords the distance test splits each curve of a segment into (`0x00BA8E90`, CONFIRMED).
const CHORDS: u32 = 32;

/// One commerce raid: a character raiding one trade segment for one target faction (the entry
/// `0x00BA76A0` → `0x00B95440` creates; the character keeps the list at +0x2D0, the segment its
/// `COMMERCE_RAIDS` list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommerceRaid {
    /// The raiding character (a force's commander).
    pub character: CharacterId,
    /// The raided segment (`TRADE_SEGMENTS[]` index, [`super::World::trade_sight`]).
    pub segment: u32,
    /// The faction whose routes on the segment the raid takes from.
    pub target: FactionId,
}

/// The cubic Bézier basis at `0x01453790` (row `j` gives control point `j`'s weight from the power
/// vector `[t³, t², t, 1]`; CONFIRMED, read from the exe).
const BEZIER_BASIS: [[f32; 4]; 4] = [[-1.0, 3.0, -3.0, 1.0], [3.0, -6.0, 3.0, 0.0], [-3.0, 3.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]];

/// `1/31` and `1/961` as the distance test's f32 constants (`0x01381958` = `0x3D042108`, `0x01381954` =
/// `0x3A886429`, CONFIRMED): `t = step / 31` and `t² = step² / 961`, each one product.
const STEP_T: f32 = f32::from_bits(0x3D04_2108);
const STEP_T2: f32 = f32::from_bits(0x3A88_6429);

/// The point of a cubic Bézier curve at chord step `step` (`t = step/31`), with the exe's f32 operations
/// in its order (CONFIRMED, `0x00BA8E90` builds the power vector, `0x007CD7E0` evaluates it): `t` and
/// `t²` each from the step by one product, `t³ = t² × t`; each weight the power vector times its basis
/// row summed left to right, and each axis the weighted control points summed left to right.
pub fn curve_point(c: &[(f32, f32); 4], step: u32) -> (f32, f32) {
    let s = step as f32;
    let t = s * STEP_T;
    let t2 = s * s * STEP_T2;
    let p = [t2 * t, t2, t, 1.0];
    let w = BEZIER_BASIS.map(|r| p[0] * r[0] + p[1] * r[1] + p[2] * r[2] + p[3] * r[3]);
    let axis = |a: [f32; 4]| w[0] * a[0] + w[1] * a[1] + w[2] * a[2] + w[3] * a[3];
    (axis(c.map(|q| q.0)), axis(c.map(|q| q.1)))
}

/// The distance from `pos` to a trade segment (`0x00BA8E90`, CONFIRMED): each curve is split into
/// 32 chords, chord `i` running from `t = i/31` to `t = (i+1)/31` (so the last one overshoots the
/// curve's end to `t = 32/31`, as the exe's divisor is one less than its count), and the distance is
/// the nearest point of any chord. `f32::MAX` (the exe's start value `0x01325DB4`) for a segment
/// without curves.
pub fn segment_distance(curves: &[[(f32, f32); 4]], pos: (f32, f32)) -> f32 {
    let mut best = f32::MAX;
    for c in curves {
        for i in 0..CHORDS {
            best = best.min(point_chord_distance(pos, curve_point(c, i), curve_point(c, i + 1)));
        }
    }
    best
}

/// The distance from `p` to the chord `a`–`b`, in the exe's f32 order (`0x00BA8E90`, CONFIRMED): the
/// projection is taken on the unit direction (`1/len` times each term), clamped to `0..=1`, and a
/// zero-length chord measures from `a`.
fn point_chord_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let (px, pz) = (p.0 - a.0, p.1 - a.1);
    let len = (dx * dx + dz * dz).sqrt();
    let u = if len > 0.0 {
        let inv = 1.0 / len;
        let u = inv * px * (dx * inv) + inv * pz * (dz * inv);
        if u < 0.0 { 0.0 } else { u.min(1.0) }
    } else {
        0.0
    };
    let (qx, qz) = (a.0 + u * dx - p.0, a.1 + u * dz - p.1);
    (qx * qx + qz * qz).sqrt()
}

/// Which kind of route a raid takes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    /// A domestic route (a trade node or port to the owner's capital).
    Domestic,
    /// An international route to `importer`.
    International {
        /// The importing faction.
        importer: FactionId,
    },
}

/// A trade route as the raids see it: its owner (the exporter), kind, path and this turn's volumes.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnedRoute<'a> {
    /// The route's owner.
    pub owner: FactionId,
    /// Domestic or international.
    pub kind: RouteKind,
    /// The route (borrowed from the world where it is stored, owned where it is built).
    pub path: Cow<'a, TradePath>,
    /// Its commodity volumes this turn ([`CampaignModel::trade_path_volumes`] for an international route,
    /// [`CampaignModel::domestic_route_volumes`] for a domestic one).
    pub volumes: Vec<u32>,
}

/// What one raiding faction takes from one route for its share (the original's rule, seam
/// [`trade.raid_take`](super::seams::RAID_TAKE)), CONFIRMED: 0 when the route is blockaded, else
/// - an international route (`ComputeTradeRouteRaidAmount` `0x00B60A40`): `Σ_c trunc((price_c ×
///   volume_c) × share)` + `trunc((resource + GDP part) × share)` + `trunc(accumulated × share)`, each
///   term truncated on its own (the resource part is 0 in every shipped file, not modelled);
/// - a domestic route (`0x00B609B0`): `Σ_c price_c × trunc(volume_c × share)`.
pub fn raid_take(model: &CampaignModel, route: &OwnedRoute<'_>, share: f32) -> i32 {
    if model.trade_path_blockaded(route.owner, &route.path) {
        return 0;
    }
    let prices = &model.world.commodity_prices;
    match route.kind {
        RouteKind::International { importer } => {
            let commodities = route
                .volumes
                .iter()
                .zip(prices)
                .map(|(v, p)| ((i64::from(*v) * i64::from(*p)) as f32 * share) as i32)
                .fold(0i32, i32::saturating_add);
            let gdp = (super::economy::trade_path_gdp_value(model, route.owner, importer, &route.path) as f32 * share) as i32;
            let accumulated = (route.path.accumulated as f32 * share) as i32;
            commodities.saturating_add(gdp).saturating_add(accumulated)
        }
        RouteKind::Domestic => route
            .volumes
            .iter()
            .zip(prices)
            .map(|(v, p)| ((*v as f32 * share) as i32).saturating_mul(*p as i32))
            .fold(0i32, i32::saturating_add),
    }
}

/// The part of a domestic route's `volume` the raiders take (`0x00B1D1F0`, CONFIRMED): over the
/// faction shares, `taken = trunc(taken + volume × share)` in single precision (`0x0126E2B0`).
pub fn raided_volume(shares: &[(FactionId, f32)], volume: u32) -> u32 {
    shares.iter().fold(0i32, |taken, (_, share)| (taken as f32 + volume as f32 * share) as i32).max(0) as u32
}

/// The raid shares on one victim's routes: every character raiding for that target, with his faction
/// and his force's share ([`CampaignModel::raid_shares_on`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RaidShares {
    /// The victim.
    pub victim: Option<FactionId>,
    /// Character → (his faction, his share).
    pub shares: BTreeMap<CharacterId, (FactionId, f32)>,
}

impl RaidShares {
    /// No raid targets the victim.
    pub fn is_empty(&self) -> bool {
        self.shares.is_empty()
    }
}

/// The network legs by their (lower, higher) node pair, as the trade manager's hash keys them
/// (`(lo ^ hi·lo ^ 0x4A545EED) % +0x1AC`, CONFIRMED by `0x00B89270`).
pub type LegIndex<'a> = BTreeMap<(u32, u32), &'a TradeLeg>;

/// The raids on one victim's routes, built once for all of them ([`CampaignModel::raid_view`]): the
/// shares on it and, when it has any, the network legs by node pair.
#[derive(Debug, Clone, Default)]
pub struct RaidView<'a> {
    /// The raid shares on the victim ([`CampaignModel::raid_shares_on`]).
    pub shares: RaidShares,
    /// The network legs ([`CampaignModel::trade_leg_index`]; empty when no raid targets the victim).
    pub legs: LegIndex<'a>,
}

impl CampaignModel {
    /// The raids on `victim`'s routes ([`RaidView`]); cheap when no raid targets it.
    pub fn raid_view(&self, victim: FactionId) -> RaidView<'_> {
        let shares = self.raid_shares_on(victim);
        let legs = if shares.is_empty() { LegIndex::new() } else { self.trade_leg_index() };
        RaidView { shares, legs }
    }

    /// The network legs by node pair ([`LegIndex`]).
    pub fn trade_leg_index(&self) -> LegIndex<'_> {
        self.world.trade_network.iter().map(|l| ((l.from.min(l.to), l.from.max(l.to)), l)).collect()
    }

    /// The trade segments of a route in walk order (CONFIRMED, `0x00B83C30`): for each waypoint but
    /// the last, the leg from its `from` to its `to` node, and each of that leg's spline ids `s`
    /// names segment `s >> 1`. A segment can appear more than once.
    pub fn route_segments(legs: &LegIndex<'_>, path: &TradePath) -> Vec<u32> {
        let n = path.waypoints.len().saturating_sub(1);
        path.waypoints[..n]
            .iter()
            .filter_map(|w| legs.get(&(w.from.min(w.to), w.from.max(w.to))))
            .flat_map(|l| l.splines.iter().map(|s| s >> 1))
            .collect()
    }

    /// The segments a route is listed on (the segments' route lists `TRADE_SEGMENTS[]` #18 / #19, which
    /// [`Self::raid_target`] and [`Self::segment_raidable`] read): the legs of [`Self::route_segments`],
    /// plus, for a sea waypoint, the legs from its start node to its region's settlement and from its end
    /// node to the next waypoint region's settlement. A segment appears once per listing.
    ///
    /// CONFIRMED by test: it gives every saved list of five vanilla saves exactly (`commerce_raid_install`,
    /// 0 of 280–287 segments differ); the exe code that fills the lists is not traced.
    pub fn registered_segments(&self, legs: &LegIndex<'_>, path: &TradePath) -> Vec<u32> {
        let settlement = |r: super::ids::RegionId| self.world.trade_node_regions.iter().find(|(_, (x, port))| *x == r && !*port).map(|(n, _)| *n);
        let mut out = Vec::new();
        let mut leg = |a: u32, b: u32| {
            if let Some(l) = legs.get(&(a.min(b), a.max(b))) {
                out.extend(l.splines.iter().map(|s| s >> 1));
            }
        };
        let n = path.waypoints.len().saturating_sub(1);
        for (i, w) in path.waypoints[..n].iter().enumerate() {
            leg(w.from, w.to);
            if w.sea {
                if let Some(s) = settlement(w.region) {
                    leg(w.from, s);
                }
                if let Some(s) = path.waypoints.get(i + 1).and_then(|x| settlement(x.region)) {
                    leg(w.to, s);
                }
            }
        }
        out
    }

    /// A domestic route's volumes this turn: with the trade nodes known, what the owner's navies gather
    /// at the route's first node ([`Self::node_supply`]); else the loaded volumes.
    pub fn domestic_route_volumes(&self, supply: Option<&[(u32, usize, u32)]>, path: &TradePath) -> Vec<u32> {
        let Some(list) = supply else { return path.volumes.clone() };
        let mut out = vec![0u32; self.world.commodity_prices.len()];
        let Some(first) = path.waypoints.first() else { return out };
        for &(_, c, v) in list.iter().filter(|(node, _, _)| *node == first.from) {
            if let Some(s) = out.get_mut(c) {
                *s = s.saturating_add(v);
            }
        }
        out
    }

    /// Every trade route of a faction: its domestic routes, then its international routes to each
    /// importer it earns from ([`super::economy::trade_route_importer_paths`]), with this turn's volumes.
    pub fn faction_trade_routes(&self, faction: FactionId) -> Vec<OwnedRoute<'_>> {
        let mut out = Vec::new();
        if let Some(domestic) = self.world.domestic_trade.get(&faction) {
            let supply = self.node_supply(faction);
            for p in domestic {
                out.push(OwnedRoute { owner: faction, kind: RouteKind::Domestic, volumes: self.domestic_route_volumes(supply.as_deref(), p), path: Cow::Borrowed(p) });
            }
        }
        let split = self.trade_split(faction);
        for (b, paths) in super::economy::trade_route_importer_paths(self, faction) {
            for p in paths.into_iter().flatten() {
                let volumes = self.trade_path_volumes(split.as_ref(), faction, b, &p);
                out.push(OwnedRoute { owner: faction, kind: RouteKind::International { importer: b }, path: Cow::Owned(p), volumes });
            }
        }
        out
    }

    /// The value a raid weighs a route by when it picks its target (`0x00BAFB20`, CONFIRMED): a
    /// domestic route's `Σ price × volume`, an international route's total
    /// ([`super::economy::trade_path_value`]: 0 when blockaded).
    pub fn raid_route_value(&self, route: &OwnedRoute<'_>) -> i32 {
        match route.kind {
            RouteKind::Domestic => self.trade_commodity_value(&route.volumes),
            RouteKind::International { importer } => super::economy::trade_path_value(self, route.owner, importer, &route.path, &route.volumes),
        }
    }

    /// The routes on each of `segments` (the segments' domestic and international route lists, which
    /// the exe keeps on the segments, `TRADE_SEGMENTS[]` #18 / #19): segment → (owner,
    /// [`Self::raid_route_value`]) per listing ([`Self::registered_segments`]), over every faction's
    /// routes ([`Self::faction_trade_routes`]).
    pub fn segment_routes(&self, segments: &BTreeSet<u32>) -> BTreeMap<u32, Vec<(FactionId, i32)>> {
        let mut out: BTreeMap<u32, Vec<(FactionId, i32)>> = BTreeMap::new();
        if segments.is_empty() {
            return out;
        }
        let legs = self.trade_leg_index();
        for f in self.world.factions.keys() {
            for r in self.faction_trade_routes(*f) {
                let on: Vec<u32> = self.registered_segments(&legs, &r.path).into_iter().filter(|s| segments.contains(s)).collect();
                if on.is_empty() {
                    continue;
                }
                let value = self.raid_route_value(&r);
                for s in on {
                    out.entry(s).or_default().push((r.owner, value));
                }
            }
        }
        out
    }

    /// Whether a segment's routes can be raided by `raider` (`0x00BA7DF0`, CONFIRMED): one of them
    /// belongs to another faction at war with it. `routes` is the segment's list of [`Self::segment_routes`].
    pub fn segment_raidable(&self, routes: &[(FactionId, i32)], raider: FactionId) -> bool {
        routes.iter().any(|(owner, _)| *owner != raider && self.world.stance(*owner, raider) == Stance::War)
    }

    /// The faction a raid on a segment takes from (`0x00BAFB20`, CONFIRMED): of the other factions at war
    /// with `raider` that own routes on it, the one whose routes there are worth most together
    /// ([`Self::raid_route_value`]); a tie goes to the lower faction id.
    pub fn raid_target(&self, routes: &[(FactionId, i32)], raider: FactionId) -> Option<FactionId> {
        let mut sums: BTreeMap<FactionId, i64> = BTreeMap::new();
        for (owner, value) in routes.iter().filter(|(o, _)| *o != raider && self.world.stance(*o, raider) == Stance::War) {
            *sums.entry(*owner).or_default() += i64::from(*value);
        }
        sums.into_iter().fold(None, |best: Option<(FactionId, i64)>, (f, v)| match best {
            Some((_, bv)) if bv >= v => best,
            _ => Some((f, v)),
        })
        .map(|(f, _)| f)
    }

    /// The trade segments within [`RAID_RADIUS`] of a position ([`segment_distance`]). INFERRED equal: the
    /// exe tests only the segments its grids list for the position's cell (`0x00BCA0F0`, grids at network
    /// +0xD0 / +0xD4, cell lookup `0x00BB8910`); the same set when each cell lists every segment within
    /// the radius of it (the grid build is not traced).
    pub fn trade_segments_near(&self, pos: (Fixed20, Fixed20)) -> BTreeSet<u32> {
        let p = (pos.0.to_f32(), pos.1.to_f32());
        self.world
            .trade_sight
            .iter()
            .enumerate()
            .filter(|(_, s)| segment_distance(&s.curves, p) < RAID_RADIUS)
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// Whether the character raids any segment (his raid list is not empty: `IsListEmpty` `0x008BC550`
    /// through its thunk `0x008BC560`).
    pub fn is_raiding(&self, character: CharacterId) -> bool {
        self.world.commerce_raids.iter().any(|r| r.character == character)
    }

    /// Ends every raid of a character (`EndCharacterCommerceRaids` `0x009DA140`, CONFIRMED): the list clear
    /// `0x008BC580`, then the price update with flag 0 (`0x00BCB020(0)` at `0x009DA160`, which rebuilds the
    /// raid takes and books them, [`Self::update_commodity_prices_with`]), then the +0xF7C dispatcher's
    /// +0x268 listener (none in the model). Its callers run it only for a raiding character (`IsListEmpty`
    /// on +0x2D0 first), so a character who does not raid runs no update.
    pub fn end_character_raids(&mut self, character: CharacterId) {
        if !self.is_raiding(character) {
            return;
        }
        self.world.commerce_raids.retain(|r| r.character != character);
        self.update_commodity_prices_with(false);
    }

    /// The character raids the trade at `pos` (`0x009D3AC0`, CONFIRMED): each segment near `pos` raidable
    /// for his faction gets a raid on its [`Self::raid_target`] (`0x00BA7680` → `0x00BA7600` →
    /// `0x00BA76A0`). His existing raids are kept: none of these ends them (the order's start does, see
    /// [`CampaignCommand::RaidTradeRoute`](super::commands::CampaignCommand::RaidTradeRoute)). The
    /// segments are taken in turn, each checked against the trade as the entries before it left it, and
    /// each new entry runs the price update with flag 0 (`AddCommerceRaidEntry` `0x00BA76A0`:
    /// `0x00BCB020(0)` after the push, CONFIRMED), which books the takes. Returns whether any segment is in
    /// range, raidable or not (with none the raid order is cancelled, `0x0094BA50` → `0x00921CD0`).
    pub fn raid_trade_route(&mut self, character: CharacterId, pos: (Fixed20, Fixed20)) -> bool {
        let Some(faction) = self.world.characters.get(&character).map(|c| c.faction) else { return false };
        let near = self.trade_segments_near(pos);
        for &segment in &near {
            let routes = self.segment_routes(&BTreeSet::from([segment]));
            let list = routes.get(&segment).map_or(&[][..], Vec::as_slice);
            if !self.segment_raidable(list, faction) {
                continue;
            }
            if let Some(target) = self.raid_target(list, faction) {
                self.world.commerce_raids.push(CommerceRaid { character, segment, target });
                self.update_commodity_prices_with(false);
            }
        }
        !near.is_empty()
    }

    /// `old`, standing at `at`, hands his forces' command to `successors` (CONFIRMED: the force's
    /// observer `0x008DB3E0`, run by the character destructor `0x0099D2D0` for a commander who dies or
    /// leaves the force; the hires and field promotions through
    /// [`Self::hand_over_raids_on_command_change`]): when he raided, his raids end (`0x009DA140`) and
    /// each new commander raids at `at`, where the old one stood ([`Self::raid_trade_route`],
    /// `0x009D3AC0` with `0x00A17D50` on the old commander, keeping any raids of his own). A commander
    /// who did not raid hands nothing on.
    pub(crate) fn hand_over_raids(&mut self, old: CharacterId, at: (Fixed20, Fixed20), successors: &[CharacterId]) {
        if !self.is_raiding(old) {
            return;
        }
        self.end_character_raids(old);
        for &c in successors {
            self.raid_trade_route(c, at);
        }
    }

    /// The commander of `force` and where he stands, when he raids: what a hire or a field promotion
    /// notes on entry (CONFIRMED in `0x00A164C0`, `0x00A16110`, `0x008E1C20` and `0x008E2260`: the
    /// force's commander `0x00898E80` on +0x74, his raid list +0x2D0 not empty, his position
    /// `0x00A17D50`), for [`Self::hand_over_raids_on_command_change`].
    pub(crate) fn raiding_commander(&self, force: ForceId) -> Option<(CharacterId, (Fixed20, Fixed20))> {
        let c = self.world.forces.get(&force)?.commander.filter(|&c| self.is_raiding(c))?;
        Some((c, self.world.characters.get(&c)?.position))
    }

    /// After a hire or a field promotion (CONFIRMED: a General hired into an army `0x00A164C0`, calls
    /// at `0x00A165D0` / `0x00A165E9`; an admiral hired into a fleet `0x00A16110`, `0x00A163CC` /
    /// `0x00A163DF`; the field promotion, land `0x008E1C20` at `0x008E2170` / `0x008E2190`, naval
    /// `0x008E2260` at `0x008E2650` / `0x008E2670`): when the commander noted on entry
    /// ([`Self::raiding_commander`]) raided and `force` is now led by someone else, his raids end and
    /// the new commander raids where he stood ([`Self::hand_over_raids`]).
    pub(crate) fn hand_over_raids_on_command_change(&mut self, raider: Option<(CharacterId, (Fixed20, Fixed20))>, force: ForceId) {
        let Some((old, at)) = raider else { return };
        let Some(new) = self.world.forces.get(&force).and_then(|f| f.commander).filter(|&n| n != old) else { return };
        self.hand_over_raids(old, at, &[new]);
    }

    /// A peace's raid step for character `c` towards the other side `other` (`0x00B262C0` → `0x00A25570`,
    /// CONFIRMED, after his siege and blockade steps): if he still raids (`0x008BC560`) and one of his raids
    /// is on `other` (`0x008CEEB0`), he loses all his raids ([`Self::end_character_raids`], `0x009DA140`,
    /// which runs the price update). A raid an earlier step's price update already ended (its raid check,
    /// [`Self::validate_commerce_raids`]) is not ended again, so it runs no update.
    pub(crate) fn end_raids_on_peace(&mut self, c: CharacterId, other: FactionId) {
        if self.world.commerce_raids.iter().any(|r| r.character == c && r.target == other) {
            self.end_character_raids(c);
        }
    }

    /// The trade update's raid check (`0x00BD1020` → `ValidateAllCommerceRaids` `0x00BA4A70` →
    /// `ValidateCharacterCommerceRaids` `0x008B3690`, CONFIRMED), so at every price update: a raider for whom
    /// one of his segments is no longer raidable ([`Self::segment_raidable`]) loses all his raids, as does
    /// a raider who no longer exists. The exe ends the first such raider through `0x00BA4A60` →
    /// `0x009DA140` ([`Self::end_character_raids`]), whose nested price update runs this check again, and
    /// then restarts its walk; ours does the same. The exe skips the check while the game flag +0xFC6 is
    /// set (UNKNOWN meaning; we always run it).
    pub fn validate_commerce_raids(&mut self) {
        while let Some(c) = self.first_invalid_raider() {
            self.end_character_raids(c);
        }
    }

    /// The first raider (in raid order) [`Self::validate_commerce_raids`] ends.
    fn first_invalid_raider(&self) -> Option<CharacterId> {
        if self.world.commerce_raids.is_empty() {
            return None;
        }
        let segments: BTreeSet<u32> = self.world.commerce_raids.iter().map(|r| r.segment).collect();
        let routes = self.segment_routes(&segments);
        self.world
            .commerce_raids
            .iter()
            .find(|r| match self.world.characters.get(&r.character) {
                None => true,
                Some(c) => !self.segment_raidable(routes.get(&r.segment).map_or(&[][..], Vec::as_slice), c.faction),
            })
            .map(|r| r.character)
    }

    /// The divisor of the raid shares (`0x00B83C30`, CONFIRMED): the largest `travel_speed` (unit record
    /// +0x40) over every unit record (the list `0x00E31890` returns), at least 1.
    pub fn max_raid_strength(&self) -> f32 {
        self.rules.units.values().map(|u| u.travel_speed as f32).fold(1.0, f32::max)
    }

    /// A unit's campaign movement points under its commander (`0x008A8670` → `0x008A86D0`, CONFIRMED):
    /// - the base is the unit's `travel_speed`;
    /// - the bonus is `mod_land_movement_campaign` (effect 137) for a land unit or
    ///   `mod_ship_movement_campaign` (72) for a ship, plus the unit-category `movement_mod` of its
    ///   category, read from the commander's view ([`Effects::character_total`]); with a bonus the
    ///   points are `round((bonus + 100) × base / 100)`, else the base;
    /// - a ship with top gallants ([`super::rules::UnitRules::top_gallants`], naval record +0x1D4) adds
    ///   `mod_ship_movement_campaign_top_gallants` (73) to the ship bonus when its commander's
    ///   `enable_top_gallants` (70), read as an int (`0x00E1F130` → `0x00E23E10`, rounded half to even),
    ///   is not 0;
    /// - the result is scaled by the force's action point factor ([`super::turn::force_action_point_factor`])
    ///   and rounded (ties to even).
    pub fn unit_campaign_moves(&self, fx: &Effects, commander: CharacterId, unit_key: &str) -> i32 {
        let Some(u) = self.rules.units.get(unit_key) else { return 0 };
        let own = if !u.is_naval {
            fx.character_total(commander, "mod_land_movement_campaign")
        } else if u.top_gallants && f64::from(fx.character_total(commander, "enable_top_gallants")).round_ties_even() != 0.0 {
            fx.character_total(commander, "mod_ship_movement_campaign") + fx.character_total(commander, "mod_ship_movement_campaign_top_gallants")
        } else {
            fx.character_total(commander, "mod_ship_movement_campaign")
        };
        let sum = own + fx.character_total_qualified(commander, BonusKind::UnitCategory, "movement_mod", &u.category);
        let base = if sum != 0.0 { ((sum + 100.0) * u.travel_speed as f32 * 0.01).round_ties_even() as i32 } else { u.travel_speed };
        let factor = super::turn::force_action_point_factor(&self.rules, fx.character_total(commander, "general_admiral_action_point_bonus"));
        (base as f32 * factor).round_ties_even() as i32
    }

    /// A raiding force's share of a route (army `0x008B6B80`, navy `0x008B6C50`, CONFIRMED): over its
    /// units, `Σ moves × 0.05 / max` ([`Self::unit_campaign_moves`], [`Self::max_raid_strength`];
    /// per unit `0x008B6C20`, in single precision). 0 for a character without a force.
    pub fn force_raid_share(&self, fx: &Effects, character: CharacterId, max: f32) -> f32 {
        let Some(force) = self.world.forces.values().find(|f| f.commander == Some(character)) else { return 0.0 };
        force.units.iter().map(|u| self.unit_campaign_moves(fx, character, &u.unit_key) as f32 * RAID_SHARE_PER_MOVE / max).sum()
    }

    /// The shares of every character raiding for `victim` ([`RaidShares`]); empty, and cheap, when no raid
    /// targets it.
    pub fn raid_shares_on(&self, victim: FactionId) -> RaidShares {
        let mut out = RaidShares { victim: Some(victim), shares: BTreeMap::new() };
        let raiders: BTreeSet<CharacterId> = self.world.commerce_raids.iter().filter(|r| r.target == victim).map(|r| r.character).collect();
        if raiders.is_empty() {
            return out;
        }
        let max = self.max_raid_strength();
        let mut effects: BTreeMap<FactionId, Effects> = BTreeMap::new();
        for c in raiders {
            let Some(f) = self.world.characters.get(&c).map(|x| x.faction) else { continue };
            let fx = effects.entry(f).or_insert_with(|| Effects::compute_for(self, f));
            out.shares.insert(c, (f, self.force_raid_share(fx, c, max)));
        }
        out
    }

    /// The raid shares on one route (`0x00B83C30` / `0x00B83360`, CONFIRMED): the characters raiding the
    /// route's segments ([`Self::route_segments`]) for its owner, once each in walk order; their shares
    /// summed per faction (in the order the factions first appear); when the faction shares sum above 1,
    /// each is divided by the sum. Empty when nobody raids the route.
    pub fn route_raid_shares(&self, view: &RaidView<'_>, owner: FactionId, path: &TradePath) -> Vec<(FactionId, f32)> {
        let shares = &view.shares;
        if shares.is_empty() || shares.victim != Some(owner) || path.waypoints.is_empty() {
            return Vec::new();
        }
        let mut seen: BTreeSet<CharacterId> = BTreeSet::new();
        let mut per_faction: Vec<(FactionId, f32)> = Vec::new();
        for s in Self::route_segments(&view.legs, path) {
            for r in self.world.commerce_raids.iter().filter(|r| r.segment == s && r.target == owner) {
                if !seen.insert(r.character) {
                    continue;
                }
                let Some(&(f, share)) = shares.shares.get(&r.character) else { continue };
                match per_faction.iter_mut().find(|(x, _)| *x == f) {
                    Some(e) => e.1 += share,
                    None => per_faction.push((f, share)),
                }
            }
        }
        let total: f32 = per_faction.iter().map(|(_, s)| *s).sum();
        if total > 1.0 {
            for e in &mut per_faction {
                e.1 /= total;
            }
        }
        per_faction
    }

    /// What each raiding faction takes from one route: its share ([`Self::route_raid_shares`]) through
    /// the [`trade.raid_take`](super::seams::RAID_TAKE) rule. Empty when nobody raids the route.
    pub fn route_raid_takes(&self, view: &RaidView<'_>, route: &OwnedRoute<'_>) -> Vec<(FactionId, i32)> {
        let shares = self.route_raid_shares(view, route.owner, &route.path);
        if shares.is_empty() {
            return Vec::new();
        }
        let rule = self.rules.seams.raid_take.rule();
        shares.into_iter().map(|(f, share)| (f, rule(self, route, share))).collect()
    }

    /// A domestic route's volumes after the raids on it, its owner's (CONFIRMED): 0 when the route is
    /// blockaded (`0x00B1D1C0`, route +0x34); else each volume less what the raiders take of it
    /// (`0x00B1D1F0`: over the route's faction → share map, which the price update's rebuild
    /// `0x00B61720` stores from [`Self::route_raid_shares`], `taken = trunc(taken + volume × share)` in
    /// single precision). This is what the route brings home: the exporter's supply in the trade split
    /// (`0x00BC26D0` at `0x00BC2D6C`) and the home term of `0x00BB3490` (at `0x00BB362F`).
    pub fn domestic_net_volumes(&self, view: &RaidView<'_>, owner: FactionId, path: &TradePath, volumes: &[u32]) -> Vec<u32> {
        if self.trade_path_blockaded(owner, path) {
            return vec![0; volumes.len()];
        }
        let shares = self.route_raid_shares(view, owner, path);
        volumes.iter().map(|&v| v.saturating_sub(raided_volume(&shares, v))).collect()
    }

    /// The sum of the takes from one international route ([`Self::route_raid_takes`]): what its owner
    /// loses (`GetTradeRouteNetValue` `0x00B79B20` = total less this).
    pub fn route_raid_loss(&self, view: &RaidView<'_>, route: &OwnedRoute<'_>) -> i32 {
        self.route_raid_takes(view, route).iter().map(|(_, t)| *t).fold(0, i32::saturating_add)
    }

    /// A faction's commerce raiding income this turn (income category 9, `finances_commerce_raiding_income`):
    /// its takes ([`Self::route_raid_takes`]) from every route, domestic and international, of each faction
    /// its raids target.
    ///
    /// Computed from the current state. What the round end pays is the value the last price update
    /// booked ([`super::economy::book_raid_income`], CONFIRMED): the update rebuilds both kinds of take, the
    /// international ones by `0x00BBD870` at `0x00BCC52F` and the domestic ones by `0x00BD1020` →
    /// `0x00B61720` at `0x00BD1B23`. PROVISIONAL: the route builder `0x00BA0490` also rebuilds the
    /// international takes outside a price update (a trade agreement `0x00B55090` → `0x00B55130`, the
    /// listeners `0x00BB9410` / `0x00BB9B90`); ours books them only at the next price update.
    pub fn raid_income(&self, faction: FactionId) -> i32 {
        let w = &self.world;
        let victims: BTreeSet<FactionId> = w
            .commerce_raids
            .iter()
            .filter(|r| w.characters.get(&r.character).is_some_and(|c| c.faction == faction))
            .map(|r| r.target)
            .collect();
        if victims.is_empty() {
            return 0;
        }
        let mut view = RaidView { shares: RaidShares::default(), legs: self.trade_leg_index() };
        let mut income = 0i32;
        for v in victims {
            view.shares = self.raid_shares_on(v);
            for route in self.faction_trade_routes(v) {
                for (f, take) in self.route_raid_takes(&view, &route) {
                    if f == faction {
                        income = income.saturating_add(take);
                    }
                }
            }
        }
        income
    }
}
