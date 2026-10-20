//! Income, upkeep and public order.
//!
//! Taxes follow the original (CONFIRMED in Ghidra, CAMPAIGN_FIDELITY.md §Economy):
//! - **Tax efficiency** ([`tax_efficiency`], 0x00BC73C0): a penalty for owning many regions,
//!   `sqrt(ln(1 + (max(min, n + admin_cost_mod) − min) / total) / ln(log_base)) × modifier × 0.1`
//!   with the `tax_efficiency_*` campaign variables (`min` = `tax_efficiency_regions_minimum`
//!   rounded to an integer, `total` = `tax_efficiency_total_regions`, `modifier` =
//!   `tax_efficiency_modifier`).
//! - **Effective rate of a class** ([`effective_tax_rate`], 0x00BA4210):
//!   `max(0, (1 + efficiency) × (rate/100 + 0.5 × character/100 + 0.5 × building/100 +
//!   0.5 × technology/100))`, 0 in a tax-exempt region.
//! - **Region taxes** ([`region_taxes`], 0x00A8C2E0 lower / 0x00AB5560 upper): each class pays
//!   `round(effective rate × (GDP + town wealth))`; both classes tax the same base.
//! - **Faction income** ([`faction_income`]): the taxes of every owned region (0x008D22F0 /
//!   0x008F9E50) plus `faction_gdp_other` (`faction_gdp_other_minor` for minor factions;
//!   0x00BBC710; picked by the major-power flag `FactionDetails::major`), minus upkeep.
//!
//! - **Trade** ([`Income::trade`]): every trade route's GDP part `trunc(2.2 × sqrt(GDP_a + GDP_b))` and
//!   accumulated value plus its commodity part `Σ volume × price` (CONFIRMED), 0 when blockaded
//!   (see [`super::trade`]); the supply is split over the partners by their net demand
//!   ([`CampaignModel::trade_split`], `0x00BC26D0`, CONFIRMED).
//!
//! The effects (traits, ministers, technologies, buildings, difficulty) come from the effects store
//! ([`super::effects`]); GDP and town wealth growth are recomputed at each round end
//! ([`recompute_region`]); public order follows the exe's slots ([`public_order_factors`]).

use super::ids::{FactionId, ForceId, RegionId};
use super::effects::{BonusKind, Effects};
use super::rules::{CampaignRules, TaxClass};
use super::world::{CampaignModel, EconomyRecord, Region, SlotRef};

/// One faction's income for one turn, split into its parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Income {
    /// Taxes from owned regions (`ECONOMICS_DATA` category 5).
    pub taxes: i32,
    /// Trade (category 7): the faction's trade routes ([`trade_routes_value`]; see the module docs).
    pub trade: i32,
    /// Commerce raiding (category 9, `finances_commerce_raiding_income`): what the faction's raids take
    /// from other factions' trade routes ([`CampaignModel::raid_income`]).
    pub raiding: i32,
    /// `faction_gdp_other` (or `faction_gdp_other_minor` for `minor` factions; category 11).
    pub other: i32,
    /// Upkeep of every unit the faction owns (a cost, so it is subtracted).
    pub upkeep: i32,
}

impl Income {
    /// Everything earned: taxes + trade + commerce raiding + other.
    pub fn revenue(&self) -> i32 {
        self.taxes.wrapping_add(self.trade).wrapping_add(self.raiding).wrapping_add(self.other)
    }

    /// Revenue − upkeep.
    pub fn net(&self) -> i32 {
        self.revenue().wrapping_sub(self.upkeep)
    }
}

/// The tax efficiency of a faction that owns `regions` regions: a fraction added to 1 in
/// [`effective_tax_rate`] (0 for few regions, negative above). `admin_cost_mod` is the faction's
/// effect of that name (added to the region count). CONFIRMED (0x00BC73C0); 0 when the rules lack
/// the `tax_efficiency_*` variables.
pub fn tax_efficiency(rules: &CampaignRules, regions: i32, admin_cost_mod: i32) -> f32 {
    let v = |k: &str| rules.variables.get(k).copied();
    let (Some(min), Some(log_base), Some(modifier), Some(total)) = (
        v("tax_efficiency_regions_minimum"),
        v("tax_efficiency_log_base"),
        v("tax_efficiency_modifier"),
        v("tax_efficiency_total_regions"),
    ) else {
        return 0.0;
    };
    // FISTP with the default rounding mode: to nearest, ties to even.
    let min = min.round_ties_even() as i32;
    let n = regions.saturating_add(admin_cost_mod).max(min);
    let x = (n - min) as f32 / total + 1.0;
    (x.ln() / log_base.ln()).sqrt() * modifier * 0.1
}

/// Tax bonuses (effect values in percent) that raise a class's effective rate.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TaxBonuses {
    /// `tax_bonus_character` of the region (or `tax_bonus_minister` in the faction's home region).
    /// Not modelled yet (0).
    pub character: f32,
    /// `tax_bonus_building`: the sum over the region's buildings.
    pub building: f32,
    /// `tax_bonus_technology` of the faction. Not modelled yet (0).
    pub technology: f32,
}

/// The effective tax rate of one class (a fraction): `rate` is the class's `taxes_levels` rate in
/// percent, `efficiency` from [`tax_efficiency`]. CONFIRMED (0x00BA4210): each bonus counts half.
pub fn effective_tax_rate(rate: i32, efficiency: f32, bonuses: TaxBonuses) -> f32 {
    let half = |pct: f32| (f64::from(pct) * 0.01 * 0.5) as f32;
    let sum = ((rate as f32) as f64 * 0.01) as f32 + half(bonuses.character) + half(bonuses.building) + half(bonuses.technology);
    let r = (1.0 + efficiency) * sum;
    if r <= 0.0 { 0.0 } else { r }
}

/// One class's taxes from one region: `round(rate × (gdp + town wealth))` (ties to even, as the
/// original's FISTP). CONFIRMED (0x00A8C2E0, 0x00AB5560).
pub fn class_taxes(effective_rate: f32, gdp: u32, town_wealth: u32) -> i32 {
    let base = gdp.wrapping_add(town_wealth) as f32;
    (effective_rate * base).round_ties_even() as i32
}

/// The number of regions a faction owns (the region list whose count the tax efficiency uses).
pub fn regions_owned(model: &CampaignModel, faction: FactionId) -> i32 {
    model.world.regions.values().filter(|r| r.owner == faction).count() as i32
}

/// The tax bonuses of a region from the effects (0x00BA4210, CONFIRMED sources):
/// `tax_bonus_minister` of the owner when the region is in its home theatre, else the region
/// governor's `tax_bonus_character`; the region's own `tax_bonus_building`; the owner's
/// `tax_bonus_technology`. PROVISIONAL: theatres are not in the model, so every region counts as
/// home (as 0-F's `effects_check`).
pub fn tax_bonuses(fx: &Effects, region: &Region) -> TaxBonuses {
    TaxBonuses {
        character: fx.faction(region.owner, "tax_bonus_minister"),
        building: fx.region_local(region.id, "tax_bonus_building"),
        technology: fx.faction(region.owner, "tax_bonus_technology"),
    }
}

/// Region tax income for one turn: the lower and the upper classes' [`class_taxes`]. A tax-exempt
/// region pays nothing. Computes the effects; use [`region_taxes_with`] to reuse them.
pub fn region_taxes(model: &CampaignModel, region: &Region) -> i32 {
    region_taxes_with(model, &Effects::compute_for(model, region.owner), region)
}

/// [`region_taxes`] with the effects already computed. The tax efficiency counts the owner's
/// `admin_cost_mod` (int effect, CONFIRMED 0x00BC73C0).
pub fn region_taxes_with(model: &CampaignModel, fx: &Effects, region: &Region) -> i32 {
    let Some(t) = region_tax_rates(model, fx, region) else { return 0 };
    [t.lower, t.upper].into_iter().map(|rate| class_taxes(rate, region.gdp, region.town_wealth)).sum()
}

/// The terms of a region's two effective tax rates, as the region details panel lists them
/// (`0x009AF570`: UpperTaxPercentage ... AdministrationCostPercentage, UpperTax, LowerTax).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RegionTaxRates {
    /// The owner's `taxes_levels` rate of the upper class, in percent.
    pub upper_level: i32,
    /// The lower class's.
    pub lower_level: i32,
    /// [`tax_efficiency`] (`0x00BC73C0`).
    pub efficiency: f32,
    /// [`tax_bonuses`] (`0x00BC7580` character / minister, `0x00BC7540` building, `0x00BC75F0`
    /// technology), in percent.
    pub bonuses: TaxBonuses,
    /// The upper class's [`effective_tax_rate`] (a fraction).
    pub upper: f32,
    /// The lower class's.
    pub lower: f32,
}

/// A region's [`RegionTaxRates`] (`0x00BA4210` per class, CONFIRMED; both rates are 0 for a
/// tax-exempt region); `None` when its owner is not in the model. The tax efficiency counts the
/// owner's `admin_cost_mod` (int effect, CONFIRMED 0x00BC73C0).
pub fn region_tax_rates(model: &CampaignModel, fx: &Effects, region: &Region) -> Option<RegionTaxRates> {
    let f = model.world.factions.get(&region.owner)?;
    let rules = &model.rules;
    let admin = fx.faction.get(&region.owner).map_or(0, |s| s.get_int("admin_cost_mod"));
    let mut t = RegionTaxRates {
        upper_level: rules.tax_rate(&f.tax_upper),
        lower_level: rules.tax_rate(&f.tax_lower),
        efficiency: tax_efficiency(rules, regions_owned(model, region.owner), admin),
        bonuses: tax_bonuses(fx, region),
        upper: 0.0,
        lower: 0.0,
    };
    if !region.tax_exempt {
        t.upper = effective_tax_rate(t.upper_level, t.efficiency, t.bonuses);
        t.lower = effective_tax_rate(t.lower_level, t.efficiency, t.bonuses);
    }
    Some(t)
}

/// Sum of one effect over a building level (0 if the level is unknown).
pub fn building_effect(rules: &CampaignRules, level: &str, effect: &str) -> f32 {
    rules.buildings.get(level).map_or(0.0, |b| b.effect(effect))
}

/// One unit's upkeep: the unit's upkeep methods, land `0x008F9B10` and naval `0x008F9D00` (CONFIRMED
/// listings). In f32: `mod = 100 + faction mod + category mod + class mod + unit-key mod`, the faction
/// mod `upkeep_cost_mod_land_all` / `upkeep_cost_mod_naval_all` read as a float, the others integer
/// reads (the exe's int read, half to even): the qualified `upkeep_mod` of the unit's category and
/// class, and the campaign's unit-key upkeep effect (`0x008F9B5F` / `0x008F9D4F`, in `spa_napoleon`
/// `guerrilla_upkeep_mod` (0x90) for a key ending in `_Guerrilla`, else `auxiliary_upkeep_mod` (0x94)
/// for `_Auxiliary`; [`super::features::CampaignFeatures::unit_key_effects`]). The result is
/// `FISTP(upkeep × mod × 0.01)`; a ship with a negative `mod` costs 0, a land unit has no such clamp.
pub fn unit_upkeep(fx: &Effects, faction: FactionId, features: &super::features::CampaignFeatures, unit_key: &str, unit: &super::rules::UnitRules) -> i32 {
    let int = |v: f32| f64::from(v).round_ties_even() as f32;
    let fmod = fx.faction(faction, if unit.is_naval { "upkeep_cost_mod_naval_all" } else { "upkeep_cost_mod_land_all" });
    let cat = int(fx.faction_qualified(faction, BonusKind::UnitCategory, "upkeep_mod", &unit.category));
    let class = int(fx.faction_qualified(faction, BonusKind::UnitClass, "upkeep_mod", &unit.unit_class));
    let key = features.effects_of_unit(unit_key).map_or(0.0, |e| int(fx.faction(faction, &e.upkeep)));
    let mult = 100.0 + fmod + cat + class + key;
    if unit.is_naval && mult < 0.0 {
        return 0;
    }
    super::commands::fistp(unit.upkeep as f32 * mult * 0.01)
}

/// What recruiting `unit_key` in `region` costs: the cost of the region's recruitable entry, which the
/// queue command charges and the queued item keeps (CONFIRMED: `0x00B58DD0` charges entry[0], the
/// list `0x00B31020` builds from the region's recruitables and prices with `0x00B0D220` over the
/// region's effect set `0x00A67530`). Computes the region's effect set; use [`recruitment_cost_in`]
/// to reuse one.
pub fn recruitment_cost(model: &CampaignModel, region: &Region, unit_key: &str, unit: &super::rules::UnitRules) -> i32 {
    recruitment_cost_in(&model.rules, &region_effect_set(model, region), unit_key, unit)
}

/// [`recruitment_cost`] over an already built region effect set (`0x00B0D220`, CONFIRMED):
/// `FISTP(((max(−100, mod) + 100) × units #7) × 0.01)` in f32, where `mod` is the sum of the
/// integer effects `recruitment_mod_cost_land_all` (or `_naval_all` for a ship), the unit category's
/// and the unit class's `cost_mod`, and the campaign's unit-key suffix effect ([`super::features::CampaignFeatures::unit_key_effects`]:
/// in `spa_napoleon` `guerrilla_cost_mod` for a key ending in `_Guerrilla`, else `auxiliary_cost_mod` for `_Auxiliary`).
/// The base is `units` #7 ([`super::rules::UnitRules::campaign_cost`]), not the #4 cost.
pub fn recruitment_cost_in(rules: &CampaignRules, set: &super::effects::EffectSet, unit_key: &str, unit: &super::rules::UnitRules) -> i32 {
    // Every lookup is the exe's int read of the effect (`0x00E23E10`, half to even).
    let int = |v: f32| f64::from(v).round_ties_even() as i32;
    let mut sum = set.get_int(if unit.is_naval { "recruitment_mod_cost_naval_all" } else { "recruitment_mod_cost_land_all" })
        + int(set.get_qualified(BonusKind::UnitCategory, "cost_mod", &unit.category))
        + int(set.get_qualified(BonusKind::UnitClass, "cost_mod", &unit.unit_class));
    if let Some(e) = rules.features.effects_of_unit(unit_key) {
        sum += set.get_int(&e.cost);
    }
    super::commands::fistp((sum.max(-100) as f32 + 100.0) * unit.campaign_cost as f32 * 0.01)
}

/// Upkeep per turn of every unit in the faction's forces (0x008B2150 over the faction's forces,
/// CONFIRMED structure): [`unit_upkeep`] of each unit, whatever its strength. Computes the effects;
/// use [`faction_upkeep_with`] to reuse them.
pub fn faction_upkeep(model: &CampaignModel, faction: FactionId) -> i32 {
    faction_upkeep_with(model, &Effects::compute_for(model, faction), faction)
}

/// [`faction_upkeep`] with the effects already computed.
pub fn faction_upkeep_with(model: &CampaignModel, fx: &Effects, faction: FactionId) -> i32 {
    let (land, naval) = faction_upkeep_split_with(model, fx, faction);
    land + naval
}

/// [`faction_upkeep_with`] split by force kind into (land, naval): the economics categories 19
/// and 20 the round end writes (`0x008E1030`: land / naval totals of `0x008B2150`, CONFIRMED).
pub fn faction_upkeep_split_with(model: &CampaignModel, fx: &Effects, faction: FactionId) -> (i32, i32) {
    let (mut land, mut naval) = (0i32, 0i32);
    for f in model.world.forces.values().filter(|f| f.faction == faction) {
        let sum: i32 = f.units.iter().map(|u| model.rules.units.get(&u.unit_key).map_or(0, |r| unit_upkeep(fx, faction, &model.rules.features, &u.unit_key, r))).sum();
        if f.is_navy { naval += sum } else { land += sum }
    }
    (land, naval)
}


/// A faction's GDP as the original sums it (0x008C3110, CONFIRMED): `faction_gdp_other` (always the
/// major value, also for minor factions) plus the GDP of every region it owns.
pub fn faction_gdp(model: &CampaignModel, faction: FactionId) -> i64 {
    let other = model.rules.var("faction_gdp_other", 0.0) as i64;
    other + model.world.regions.values().filter(|r| r.owner == faction).map(|r| i64::from(r.gdp)).sum::<i64>()
}

/// The factions `faction` trades with by agreement, the ones the supply split shares to
/// ([`CampaignModel::trade_split`]): every faction it has a trade agreement with
/// (`DIPLOMACY_RELATIONSHIP` #2) and is not at war with. PROVISIONAL: the original also needs a
/// land or sea path between the two (the route builder 0x00BC0960 / 0x00BC0DC0, with the
/// `trade_route_cap_land/sea` limits) and drops routes that are blockaded (0x00B12050); every
/// agreement of the shipped start positions has a route (their stored trade figures match).
/// The factions a route of `faction` earns from are [`trade_route_importers`].
pub fn trade_partners(model: &CampaignModel, faction: FactionId) -> Vec<FactionId> {
    model.world.relationships.keys().filter(|(a, b)| *a == faction && is_trade_partner(model, *a, *b)).map(|(_, b)| *b).collect()
}

/// Whether `b` is one of `a`'s [`trade_partners`].
pub fn is_trade_partner(model: &CampaignModel, a: FactionId, b: FactionId) -> bool {
    model.world.trade_held(a, b)
        && model.world.factions.contains_key(&b)
        && model.world.stance(a, b) != super::Stance::War
}

/// The importers `faction`'s routes earn from (`0x00BB3490`, CONFIRMED): the exe sums every route in
/// the exporter's entry (trade manager map +0x11C), with no agreement or war check. So besides its
/// [`trade_partners`] (each with its stored routes or, without one, a built route), this lists every
/// importer it still has a stored route to: a broken agreement removes only the first route each way
/// (`0x00BC0120`), and the rest stay and keep earning (their volumes are 0, as the split clears every
/// route and shares only to partners, [`CampaignModel::trade_path_volumes`]). Sorted, no repeats.
pub fn trade_route_importers(model: &CampaignModel, faction: FactionId) -> Vec<FactionId> {
    let mut out: Vec<FactionId> = model
        .world
        .trade_paths
        .range((faction, FactionId(i32::MIN))..=(faction, FactionId(i32::MAX)))
        .filter(|(_, p)| !p.is_empty())
        .map(|((_, b), _)| *b)
        .collect();
    out.extend(trade_partners(model, faction));
    out.sort_unstable();
    out.dedup();
    out
}

/// Each importer of [`trade_route_importers`] with the pair's routes ([`CampaignModel::trade_routes_of`]),
/// built once for the income and the home term to share.
pub type ImporterRoutes = Vec<(FactionId, Option<Vec<super::trade::TradePath>>)>;

/// [`trade_route_importers`] of `faction`, each with its routes ([`ImporterRoutes`]).
pub fn trade_route_importer_paths(model: &CampaignModel, faction: FactionId) -> ImporterRoutes {
    trade_route_importers(model, faction).into_iter().map(|b| (b, model.trade_routes_of(faction, b))).collect()
}

/// The GDP part of a trade route's value (0x00B3F8F0, CONFIRMED):
/// `trunc(trade_route_value_combined_gdp_proportion × sqrt(GDP_a + GDP_b))`.
pub fn trade_route_gdp_value(model: &CampaignModel, a: FactionId, b: FactionId) -> i32 {
    let p = model.rules.var("trade_route_value_combined_gdp_proportion", 0.0);
    let sum = (faction_gdp(model, a) + faction_gdp(model, b)).max(0) as f32;
    (p * sum.sqrt()) as i32
}

/// The GDP part of one route (0x00B3F8F0): the factions are the owners of the route's first and last
/// waypoint regions (INFERRED: in `auto_after_c8` both routes between Württemberg and the landless
/// Bavaria carry 296 = trunc(2.2 × sqrt(GDP Austria + GDP Württemberg)); Austria owns eur_bavaria,
/// where they start or end). Falls back to the pair (`a`, `b`) when a waypoint region is unknown.
pub fn trade_path_gdp_value(model: &CampaignModel, a: FactionId, b: FactionId, path: &super::trade::TradePath) -> i32 {
    let owner = |w: Option<&super::trade::TradeWaypoint>| w.and_then(|w| model.world.regions.get(&w.region)).map(|r| r.owner);
    let ea = owner(path.waypoints.first()).unwrap_or(a);
    let eb = owner(path.waypoints.last()).unwrap_or(b);
    trade_route_gdp_value(model, ea, eb)
}

/// The value of one (exporter, importer) pair's routes this turn: per route (`0x00B15B50`,
/// CONFIRMED) 0 if blockaded, else its commodity part (the volumes of [`CampaignModel::trade_path_volumes`])
/// plus the GDP part ([`trade_route_gdp_value`]) plus its own accumulated value
/// ([`TradePath::accumulated`](super::trade::TradePath::accumulated): the recompute copies +0x3C into
/// +0x38 and adds it to the total +0x28). A pair without a loaded route gets one built over the trade
/// network ([`CampaignModel::build_trade_route`]); without a path it has no route and earns nothing.
/// Without a loaded network (tests) such a pair counts its GDP part and its route's accumulated value
/// ([`World::trade_accumulated`](super::World::trade_accumulated)). The resource part is 0 in every
/// shipped file (not modelled).
///
/// The income reads each route's net value (`GetTradeRouteNetValue` 0x00B79B20, CONFIRMED): the total
/// less the commerce-raid takes in its list (+0x58..+0x64, {faction, amount} entries), which the
/// recompute refills each time (`0x00B15B50` → `0x00B61780` → `0x00BA7730` → `0x00B83C30`,
/// [`CampaignModel::route_raid_takes`]).
pub fn trade_pair_value(model: &CampaignModel, a: FactionId, b: FactionId) -> i32 {
    trade_pair_value_with(model, model.trade_split(a).as_ref(), &model.raid_view(a), a, b, model.trade_routes_of(a, b).as_deref())
}

/// One international route's total (`0x00B15B50`, CONFIRMED; +0x28): 0 when blockaded, else its
/// commodity part for `volumes` plus its GDP part ([`trade_path_gdp_value`]) plus its accumulated value.
pub fn trade_path_value(model: &CampaignModel, a: FactionId, b: FactionId, path: &super::trade::TradePath, volumes: &[u32]) -> i32 {
    if model.trade_path_blockaded(a, path) {
        return 0;
    }
    model.trade_commodity_value(volumes).saturating_add(trade_path_gdp_value(model, a, b, path)).saturating_add(path.accumulated)
}

/// The exporter's supply split ([`CampaignModel::trade_split`]): importer → volumes.
type Split = std::collections::BTreeMap<FactionId, Vec<u32>>;

/// [`trade_pair_value`] with the exporter's supply split, the raids on its routes
/// ([`CampaignModel::raid_view`], built once for all its importers) and the pair's routes
/// ([`CampaignModel::trade_routes_of`]) given.
pub fn trade_pair_value_with(
    model: &CampaignModel,
    split: Option<&Split>,
    raids: &super::commerce_raid::RaidView<'_>,
    a: FactionId,
    b: FactionId,
    routes: Option<&[super::trade::TradePath]>,
) -> i32 {
    match routes {
        None if !model.world.trade_network.is_empty() => 0,
        None => trade_route_gdp_value(model, a, b).saturating_add(model.world.trade_accumulated.get(&(a, b)).copied().unwrap_or(0)),
        Some(paths) => paths
            .iter()
            .map(|p| {
                let volumes = model.trade_path_volumes(split, a, b, p);
                let value = trade_path_value(model, a, b, p, &volumes);
                if raids.shares.is_empty() {
                    return value;
                }
                let kind = super::commerce_raid::RouteKind::International { importer: b };
                let route = super::commerce_raid::OwnedRoute { owner: a, kind, path: std::borrow::Cow::Borrowed(p), volumes };
                value.saturating_sub(model.route_raid_loss(raids, &route))
            })
            .fold(0i32, i32::saturating_add),
    }
}

/// The value of all of a faction's trade routes for one turn (`GetFactionInternationalTradeIncome`
/// 0x00BB3490, CONFIRMED): the sum of [`trade_pair_value`] over every importer its routes go to
/// ([`trade_route_importers`]: the exe adds up every route of the exporter, with or without an
/// agreement), plus the commodities it brings home but does not export (the `spa_france` special case,
/// [`CampaignModel::trade_home_value`]).
pub fn trade_routes_value(model: &CampaignModel, faction: FactionId) -> i32 {
    let split = model.trade_split(faction);
    let routes = trade_route_importer_paths(model, faction);
    let home = model.trade_home_value(faction, split.as_ref(), &routes);
    let raids = model.raid_view(faction);
    routes
        .iter()
        .map(|(b, paths)| trade_pair_value_with(model, split.as_ref(), &raids, faction, *b, paths.as_deref()))
        .fold(home, i32::saturating_add)
}

/// Round end: every open trade route of the faction accumulates
/// `trunc((1 + trade_route_all_mod_growth_rate/100) × trade_route_value_accumulator_proportion × value)`
/// with `value` = commodity + resource + GDP part (`AccumulateTradeRouteValue` 0x00B05CC0, CONFIRMED),
/// over the same routes that earn ([`trade_route_importers`]: a route left after a broken agreement
/// keeps its GDP part, as `RecomputeAllInternationalTradeRoutes` 0x00BBD870 recomputes every route).
/// Each route adds its own share to its own value (+0x3C, [`TradePath::accumulated`](super::trade::TradePath::accumulated));
/// a pair's built route (no stored route) adds to [`World::trade_accumulated`](super::World::trade_accumulated).
pub fn accumulate_trade(model: &mut CampaignModel, faction: FactionId) {
    let fx = Effects::compute_for(model, faction);
    accumulate_trade_with(model, &fx, faction);
}

/// [`accumulate_trade`] with the faction's effects already computed.
pub fn accumulate_trade_with(model: &mut CampaignModel, fx: &Effects, faction: FactionId) {
    let growth = fx.faction(faction, "trade_route_all_mod_growth_rate");
    let p = (growth * 0.01 + 1.0) * model.rules.var("trade_route_value_accumulator_proportion", 0.0);
    // (importer, index of the stored route or None for the pair's built route, share)
    let mut adds: Vec<(FactionId, Option<usize>, i32)> = Vec::new();
    let split = model.trade_split(faction);
    for b in trade_route_importers(model, faction) {
        let stored = model.world.trade_paths.get(&(faction, b)).is_some_and(|ps| !ps.is_empty());
        match model.trade_routes_of(faction, b) {
            None if !model.world.trade_network.is_empty() => {}
            None => adds.push((b, None, (p * trade_route_gdp_value(model, faction, b) as f32) as i32)),
            Some(paths) => {
                for (i, path) in paths.iter().enumerate().filter(|(_, path)| !model.trade_path_blockaded(faction, path)) {
                    let value = model.trade_commodity_value(&model.trade_path_volumes(split.as_ref(), faction, b, path)).saturating_add(trade_path_gdp_value(model, faction, b, path));
                    adds.push((b, stored.then_some(i), (p * value as f32) as i32));
                }
            }
        }
    }
    for (b, index, add) in adds {
        let slot = match index {
            Some(i) => model.world.trade_paths.get_mut(&(faction, b)).and_then(|ps| ps.get_mut(i)).map(|p| &mut p.accumulated),
            None => Some(model.world.trade_accumulated.entry((faction, b)).or_insert(0)),
        };
        if let Some(v) = slot {
            *v = v.saturating_add(add);
        }
    }
}

/// One faction's income for its next turn. An unknown faction earns nothing.
pub fn faction_income(model: &CampaignModel, faction: FactionId) -> Income {
    faction_income_with(model, &Effects::compute_for(model, faction), faction)
}

/// [`faction_income`] with the effects already computed.
pub fn faction_income_with(model: &CampaignModel, fx: &Effects, faction: FactionId) -> Income {
    faction_income_with_upkeep(model, fx, faction, || faction_upkeep_with(model, fx, faction))
}

/// [`faction_income_with`] with the upkeep supplied (a caller that already has the unit upkeep passes it,
/// so the forces are walked once). `upkeep` is not called for a faction that does not exist.
pub fn faction_income_with_upkeep(model: &CampaignModel, fx: &Effects, faction: FactionId, upkeep: impl FnOnce() -> i32) -> Income {
    if !model.world.factions.contains_key(&faction) {
        return Income::default();
    }
    let taxes = faction_taxes_with(model, fx, faction);
    let other = other_income(model, faction);
    let trade = trade_routes_value(model, faction);
    let raiding = model.raid_income(faction);
    Income { taxes, trade, raiding, other, upkeep: upkeep() }
}

/// A faction's other income (category 11, which `0x00BBC710` writes at the faction's load,
/// `0x0087AD51`): campaign variable `faction_gdp_other`, or `faction_gdp_other_minor` for a faction
/// that is not a major power; 0 for the rebels and an unknown faction.
pub fn other_income(model: &CampaignModel, faction: FactionId) -> i32 {
    let Some(f) = model.world.factions.get(&faction) else { return 0 };
    if model.world.is_rebel_faction(faction) {
        return 0;
    }
    // CONFIRMED: the major-power flag (faction +0x524, `FactionDetails::major`) picks the variable.
    // Without loaded details (tests, tools) the `factions` category stands in (INFERRED).
    let minor = match model.world.faction_details.get(&faction).and_then(|d| d.major) {
        Some(major) => !major,
        None => model.rules.faction_categories.get(&f.key).is_some_and(|c| c == "minor"),
    };
    if minor {
        model.rules.var("faction_gdp_other_minor", model.rules.var("faction_gdp_other", 0.0)) as i32
    } else {
        model.rules.var("faction_gdp_other", 0.0) as i32
    }
}

/// A faction's taxes for the turn: [`region_taxes_with`] summed over the regions it owns (the upper
/// classes' `0x00AB5560` by `0x008F9E50` plus the lower classes' `0x00A8C2E0` by `0x008D22F0`, CONFIRMED).
pub fn faction_taxes_with(model: &CampaignModel, fx: &Effects, faction: FactionId) -> i32 {
    model.world.regions.values().filter(|r| r.owner == faction).map(|r| region_taxes_with(model, fx, r)).sum()
}

/// One income category of a faction's running economics record that `0x00BBD7C0` writes from the
/// current state (its argument 0 or 2; argument 1, category 6, builds a vector through `0x00AA25C0`,
/// discards it and writes 0: CONFIRMED by disassembly, and 0 in every record of the vanilla saves).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunningIncome {
    /// Category 5, taxes ([`faction_taxes_with`]: `0x008F9E50` + `0x008D22F0`).
    Taxes,
    /// Category 7, trade ([`trade_routes_value`], `GetFactionInternationalTradeIncome` `0x00BB3490`).
    Trade,
}

/// `faction`'s running economics record: the last of its history (record `(cursor − 1) mod 10`,
/// `0x00BABE00`), which the round end pays and [`super::World::last_income`] reads. A faction
/// without one gets a new record first: the exe's economics always holds one
/// (`ConstructNewFactionEconomics` `0x00B96100` starts with one, CONFIRMED), into which the new-faction
/// path writes the other income (`0x0087BD70` → `0x00BBC710`, CONFIRMED). `None` for an unknown faction.
fn running_record(model: &mut CampaignModel, faction: FactionId) -> Option<&mut EconomyRecord> {
    if !model.world.factions.contains_key(&faction) {
        return None;
    }
    if model.world.economy_history.get(&faction).is_none_or(|h| h.is_empty()) {
        let mut first = [0; 25];
        first[11] = other_income(model, faction);
        model.world.economy_history.entry(faction).or_default().push(first);
    }
    model.world.economy_history.get_mut(&faction)?.last_mut()
}

/// Writes one income category of `faction`'s running record from the current state, as
/// `0x00BBD7C0` does (its callers, CONFIRMED): taxes when a tax rate changes (`0x00BC5380` lower,
/// `0x00BC5440` upper) and when a region changes owner (`TransferRegionToFaction` `0x00A64AC0`, old
/// and new owner, the rebels too); trade after each trade update (`0x00BCB020` at `0x00BCE78E`,
/// every listed faction); all three at the round start ([`refresh_running_record_round_start`]) and
/// at the round end ([`settle_round`]). The exe's other caller, a region's tax exemption changing
/// (`0x00AAA140`), has no counterpart: the model never changes an exemption.
pub fn refresh_running_income(model: &mut CampaignModel, faction: FactionId, category: RunningIncome) {
    let (index, value) = match category {
        RunningIncome::Taxes => (5, faction_taxes_with(model, &Effects::compute_for(model, faction), faction)),
        RunningIncome::Trade => (7, trade_routes_value(model, faction)),
    };
    if let Some(r) = running_record(model, faction) {
        r[index] = value;
    }
}

/// A faction's part of the round start for its running record (`RunFactionRoundStart` `0x008F1F60`
/// step 1d, CONFIRMED; not for the rebels or a faction out of the game, which the caller skips): the
/// land and naval upkeep ([`faction_upkeep_split_with`], `0x008B2150`) into the trackers +0x738 / +0x744
/// ([`set_upkeep_trackers`]), then taxes, category 6 and trade (`0x00BC7080` → `0x00BBD7C0(0..2)`).
pub fn refresh_running_record_round_start(model: &mut CampaignModel, faction: FactionId) {
    let fx = Effects::compute_for(model, faction);
    let (land, naval) = faction_upkeep_split_with(model, &fx, faction);
    set_upkeep_trackers(model, faction, land, naval);
    let taxes = faction_taxes_with(model, &fx, faction);
    let trade = trade_routes_value(model, faction);
    if let Some(r) = running_record(model, faction) {
        (r[5], r[6], r[7]) = (taxes, 0, trade);
    }
}

/// Adds `amount` to expense category `18 + group` of `faction`'s running record (`0x00BBE930(amount,
/// group)`, CONFIRMED: an integer add, the converter the identity). Nothing for an unknown faction.
fn book_expense(model: &mut CampaignModel, faction: FactionId, group: usize, amount: i32) {
    if let Some(r) = running_record(model, faction) {
        r[18 + group] = r[18 + group].wrapping_add(amount);
    }
}

/// Adds `amount` to income category `8 + group` of `faction`'s running record (`0x00BBE9A0(amount,
/// group)`, CONFIRMED: an integer add, the converter the identity). Nothing for an unknown faction.
fn book_income(model: &mut CampaignModel, faction: FactionId, group: usize, amount: i32) {
    if let Some(r) = running_record(model, faction) {
        r[8 + group] = r[8 + group].wrapping_add(amount);
    }
}

/// The treaty money of `owner`'s relationship towards `target` as its trackers book it, `sign` times
/// (+1 after a change, −1 before one; every writer of the payments, the tribute or the income brackets
/// its change with the two). CONFIRMED trackers:
/// - each regular payment ([`super::details::RegularPayment`], 24 bytes on the payer's record) is an
///   expense of group 4 (category 22) of the owner, its payer, and an income of group 0 (category 8)
///   of the target, its payee: built by `0x00AE90A0` (`0x00AE5570` books the payer's line, then the
///   payee's), copied by `0x00AE5540`, taken back by its destructor `0x0086A120` (the payee's, then the
///   payer's through `0x0088CB40`);
/// - the protectorate tribute (record +0x7AC, the owner its payer) an expense of group 6 (category 24)
///   of the owner (`0x00AE55D0` in the record's constructor, setter `0x00AF8130`: new − old);
/// - the patron income (record +0x7B8 / +0x7BC, the owner its receiver) an income of group 2
///   (category 10) of the owner (`0x00BBE9A0(new − old, 2)` at each write).
///
/// A zero amount books nothing, so a faction without a history gets no record from it.
pub(super) fn book_treaty_money(model: &mut CampaignModel, owner: FactionId, target: FactionId, sign: i32) {
    let Some(r) = model.world.relationships.get(&(owner, target)) else { return };
    let (paid, tribute, income) = (r.payments_total(), r.protectorate_tribute, r.protectorate_income);
    for (faction, income_side, group, amount) in [(owner, false, 4, paid), (target, true, 0, paid), (owner, false, 6, tribute), (owner, true, 2, income)] {
        if amount == 0 {
            continue;
        }
        let amount = amount.wrapping_mul(sign);
        if income_side {
            book_income(model, faction, group, amount);
        } else {
            book_expense(model, faction, group, amount);
        }
    }
}

/// The tribute a protectorate pays its patron, which is also the patron's income from it
/// (`0x00BBCD00`, CONFIRMED): categories 5 + 6 + 7 (taxes, 6, trade; `0x00B803A0` sums the record's
/// i32s from +0x14 to +0x20, wrapping) of `protectorate`'s running record (`0x00BABE00`), divided by 5
/// toward zero (the multiply by `0x66666667` with the sign fix). Both the tribute's and the income's
/// writers read the protectorate's record. 0 without a history (the exe's fresh record has no taxes or
/// trade yet).
pub(super) fn protectorate_tribute_value(model: &CampaignModel, protectorate: FactionId) -> i32 {
    model.world.last_economy_record(protectorate).map_or(0, |r| super::world::economy_sum(r, 5..8) / 5)
}

/// Sets `faction`'s land and naval upkeep trackers ([`super::World::upkeep_booked`]): each books the
/// change, new − booked, into its running record's category 19 / 20 and keeps the new amount
/// (`0x0088F950` / `0x0088F990`, CONFIRMED: the change is booked when the tracker is linked to its
/// faction, which every faction's is from its load or creation on). Callers: the round start
/// ([`refresh_running_record_round_start`]) and the round end's first pass
/// ([`CampaignModel::prepare_round_end`]), as the exe's (`0x008F2048` / `0x008F2057`, `0x008E10BA` /
/// `0x008E10C9`).
pub fn set_upkeep_trackers(model: &mut CampaignModel, faction: FactionId, land: i32, naval: i32) {
    if !model.world.factions.contains_key(&faction) {
        return;
    }
    let booked = model.world.upkeep_booked.get(&faction).copied().unwrap_or([0, 0]);
    book_expense(model, faction, 1, land.wrapping_sub(booked[0]));
    book_expense(model, faction, 2, naval.wrapping_sub(booked[1]));
    model.world.upkeep_booked.insert(faction, [land, naval]);
}

/// Sets `region`'s policing tracker ([`super::population::PopulationState::policing_booked`]) to
/// `amount`: the change is booked into its owner's category 21 (`0x00A51230`, CONFIRMED: new − old
/// through `0x00BBE930(_, 3)` when linked to an owner, which every region's is from its load on).
pub fn set_policing_tracker(model: &mut CampaignModel, region: RegionId, amount: i32) {
    let Some(r) = model.world.regions.get_mut(&region) else { return };
    let change = amount.wrapping_sub(r.population_state.policing_booked);
    r.population_state.policing_booked = amount;
    let owner = r.owner;
    book_expense(model, owner, 3, change);
}

/// `region`'s policing tracker follows an owner change (`TransferRegionToFaction` `0x00A64AC0` →
/// `0x00AA16E0` → `0x00AAF220` at `0x00A65103`, CONFIRMED): its amount leaves the old owner's
/// category 21 and joins the new owner's. Never reached for the same owner (`0x00A64AC0` returns at `0x00A64D13` first).
pub fn move_policing_tracker(model: &mut CampaignModel, region: RegionId, old: FactionId, new: FactionId) {
    let Some(amount) = model.world.regions.get(&region).map(|r| r.population_state.policing_booked) else { return };
    book_expense(model, old, 3, amount.wrapping_neg());
    book_expense(model, new, 3, amount);
}

/// The script's `other_income_mod(faction, amount)` (`0x0097BB70` → `0x00A15B30`, CONFIRMED). The Lua
/// number is rounded as `treasury_mod`'s is (`0x010556D0`, FISTP). A positive amount is a lasting entry
/// of income group 3: `amount` more in category 11 of `faction`'s running record, carried into every
/// later record. A negative one, for a faction no human plays (a human's is dropped at `0x0097BBE3`), is
/// an entry of expense group 0: `−amount` more in category 18. 0 or an unknown faction does nothing.
/// The entries are not saved (CAMPAIGN_DATA.md §3): a load books the record without them
/// ([`rebuild_running_record`]) and the shipped scripts add them again (spa_napoleon's `OnModelCreated`
/// from its saved `VAR_other_income_boost_*`).
pub fn script_other_income_mod(model: &mut CampaignModel, faction: FactionId, amount: f32) {
    let amount = super::commands::fistp(amount);
    if amount > 0 {
        if let Some(r) = running_record(model, faction) {
            r[11] = r[11].wrapping_add(amount);
        }
    } else if amount < 0 && !model.is_human(faction) {
        book_expense(model, faction, 0, amount.wrapping_neg());
    }
}

/// `faction`'s running record as the exe's load leaves it (`ConstructFactionEconomicsFromSave`
/// `0x00B95B90` zero-fills categories 8..11 and 18..24, CONFIRMED; then the objects that track them book
/// their amounts again as they load): 11 the other income ([`other_income`], `0x00BBC710` at `0x0087AD51`),
/// 19 / 20 the upkeep trackers (`0x0088F950` / `0x0088F990` at `0x0087B548` / `0x0087B551`), 21 the
/// policing trackers of the regions it owns (`0x00A456D0` → `0x00A51230`, then `0x00AAF220` when the
/// region is linked to its owner), 9 the commerce raids' income ([`CampaignModel::raid_income`]; the
/// live raid entries book their takes into group 1 and take them back when dropped, `0x00BBE9A0` /
/// `0x00AC8810`, CONFIRMED, so 9 holds the sum of the takes). 8 / 10 / 22 / 24 the treaty money of the
/// relationships (the record copy constructor `0x00AE9160` books each regular payment into its payer's 22
/// and payee's 8, each tribute into its owner's 24 and each patron income into its owner's 10, CONFIRMED;
/// [`book_treaty_money`]). The script's `other_income_mod` entries
/// (11 and 18, [`script_other_income_mod`]; 18's only writers, `0x00984B30` / `0x00984B60` / `0x0099C2C0`)
/// are not saved, so the load drops them as the exe's does. A faction without a history gets its record
/// first ([`running_record`]), so later tracker changes add to the full base amounts; an unknown
/// faction is left alone.
pub fn rebuild_running_record(model: &mut CampaignModel, faction: FactionId) {
    let raiding = model.raid_income(faction);
    let other = other_income(model, faction);
    let [land, naval] = model.world.upkeep_booked.get(&faction).copied().unwrap_or([0, 0]);
    let policing = model.world.regions.values().filter(|r| r.owner == faction).fold(0i32, |a, r| a.wrapping_add(r.population_state.policing_booked));
    let (mut paid_in, mut tribute_in, mut paid_out, mut tribute_out) = (0i32, 0i32, 0i32, 0i32);
    for (&(owner, target), r) in &model.world.relationships {
        if owner == faction {
            paid_out = paid_out.wrapping_add(r.payments_total());
            tribute_out = tribute_out.wrapping_add(r.protectorate_tribute);
            tribute_in = tribute_in.wrapping_add(r.protectorate_income);
        }
        if target == faction {
            paid_in = paid_in.wrapping_add(r.payments_total());
        }
    }
    if let Some(r) = running_record(model, faction) {
        r[8..12].fill(0);
        r[18..25].fill(0);
        (r[8], r[9], r[10], r[11]) = (paid_in, raiding, tribute_in, other);
        (r[19], r[20], r[21], r[22], r[24]) = (land, naval, policing, paid_out, tribute_out);
    }
}

impl CampaignModel {
    /// The round end's first pass (`0x00948CF0` → `PrepareFactionRoundEnd` `0x008E1030` per listed faction,
    /// CONFIRMED), before any faction's economy: the population factors of every listed faction's regions
    /// ([`Self::refresh_population_factors`]); then, for each listed faction still in the game, not the
    /// rebels, the upkeep trackers set to its land and naval upkeep (`0x008B2150(…, 1)` → `0x0088F950` /
    /// `0x0088F990`, [`set_upkeep_trackers`]) and each owned region's policing tracker to its policing cost
    /// (`0x00AB4400` → `0x00AB43A0`: `0x00A51230` with the cost of a freshly refreshed copy, the cost the
    /// refresh just stored; [`set_policing_tracker`]). The pass's last step, the cannot-pay flag
    /// (`0x00BBC7D0` on the running record's sums), is [`settle_round`]'s check.
    pub fn prepare_round_end(&mut self) {
        self.refresh_population_factors();
        let factions: Vec<FactionId> = self.world.factions_in_turn_order().into_iter().filter(|&f| !self.is_rebel_faction(f) && self.in_the_game(f)).collect();
        for faction in factions {
            let fx = Effects::compute_for(self, faction);
            let (land, naval) = faction_upkeep_split_with(self, &fx, faction);
            set_upkeep_trackers(self, faction, land, naval);
            let regions: Vec<(RegionId, i32)> = self.world.regions.values().filter(|r| r.owner == faction).map(|r| (r.id, r.population_state.policing_cost)).collect();
            for (region, cost) in regions {
                set_policing_tracker(self, region, cost);
            }
        }
    }
}

/// The record the round end starts after paying `paid` (`0x00BB8EE0` → `0x00B955C0`, CONFIRMED):
/// categories 8..11 and 18..24 carried over (the running income and expenses their trackers keep up,
/// `0x00BBE930`), the one-off groups 0..4 and 12..17 and categories 5..7 cleared; then the end of
/// `0x00BABE30` writes taxes, category 6 and trade again (`0x00BBD7C0(0..2)` at `0x00BAC093`).
/// Nothing those read changes between the payment and that write, so they are `paid`'s.
pub fn next_running_record(paid: &EconomyRecord) -> EconomyRecord {
    let mut next = [0; 25];
    next[8..12].copy_from_slice(&paid[8..12]);
    next[18..25].copy_from_slice(&paid[18..25]);
    (next[5], next[6], next[7]) = (paid[5], 0, paid[7]);
    next
}

/// The `gdp_*` effects a building adds to its region's GDP, as `building_effects_junction` keys
/// (mapped by `effect_bonus_value_basic_junction` to gdp_farm 4, gdp_industry 5, gdp_mine 6 and
/// gdp_port_trade 8, the effects 0x00A6C4E0 sums).
pub const SLOT_GDP_EFFECTS: [&str; 4] = ["gdp_farm", "gdp_industry", "gdp_mine", "gdp_port"];

/// The town wealth growth effects of a building (0x00A6C570: tw_growth_education 52, government 53,
/// home_region 54, industry 55, port 57, roads 58; DB keys).
pub const SLOT_TW_EFFECTS: [&str; 6] = [
    "tw_growth_education",
    "tw_growth_government",
    "tw_growth_ministers_home_theatre",
    "tw_growth_industry",
    "tw_growth_port",
    "tw_growth_roads",
];

/// An effect of the region's effect bundle: the sum of a DB effect key over the region's buildings,
/// the governing faction's government type and its tax levels (`taxes_effects_jct`, both classes).
/// The faction-wide part (faction-wide buildings, technologies, ministers, difficulty) is added by
/// the callers from the effects store ([`recompute_region_with`]).
///
/// The **governing** faction, not the owner (`0x00A67530`, the same set [`region_effect_set`] builds
/// for the public order; CONFIRMED for `eur_bavaria`, whose slots Bavaria holds while Austria owns it:
/// with Austria's faction-wide technology the GDP is 1277, Bavaria's gives the stored 1275), and a
/// tax-exempt region takes no tax bundle at all (`REGION` #19, the same guard [`region_effect_set`]
/// has; CONFIRMED: `eur_moravia` and `eur_wallachia` store their growth without the tax term).
pub fn region_effect(model: &CampaignModel, region: &Region, key: &str) -> f32 {
    let rules = &model.rules;
    let mut v: f32 = region.buildings().map(|b| building_effect(rules, &b.level_key, key)).sum();
    if let Some(f) = model.world.factions.get(&governing_faction(model, region)) {
        let sum = |list: Option<&Vec<(String, f32)>>| list.map_or(0.0, |l| l.iter().filter(|(k, _)| k == key).map(|(_, x)| *x).sum());
        v += sum(rules.government_effects.get(&f.government_key));
        if !region.tax_exempt {
            for (class, level) in [(TaxClass::Lower, &f.tax_lower), (TaxClass::Upper, &f.tax_upper)] {
                v += sum(rules.effects.tax_rows(class.key(), level));
            }
        }
    }
    v
}

/// The faction whose faction-wide sum and government a region's own effect set reads
/// (`0x00A67530`, CONFIRMED through `region_effect_set`): the region's **governing** faction, which is
/// its owner unless another faction holds the governorship that lists it (`eur_bavaria`).
fn governing_faction(model: &CampaignModel, region: &Region) -> FactionId {
    model.world.governing_faction(region.id).unwrap_or(region.owner)
}

/// One building's effect as the effect getter returns it as an integer (0x00E1F130 / 0x00E23E10:
/// rounded to nearest, ties to even).
fn int_effect(rules: &CampaignRules, level: &str, key: &str) -> i32 {
    building_effect(rules, level, key).round_ties_even() as i32
}

/// A region's GDP and town wealth growth, recomputed as the original does every round
/// (0x00A6AFC0, CONFIRMED; CAMPAIGN_FIDELITY.md §GDP):
/// ```text
/// gdp    = base GDP + Σ undamaged buildings Σ_gdp effects trunc(effect × (1 + gdp_mod_all/100))
/// growth = tw_growth_factionwide + discontent term (#18)
///        + Σ undamaged buildings Σ_tw effects trunc(effect × (1 + tw_growth_mod_all/100))
///        + tw_growth_technologies
/// growth = trunc(trunc((m − #17) × growth) + growth + fixed)
/// ```
/// with `m` = the region's `tw_growth_taxes_modifier` when `growth ≥ 0` (else 0) and `fixed` =
/// `tw_growth_taxes_fixed` (the tax level effects of both classes; a tax-exempt region has none). The
/// government's and the faction-wide part are the **governing** faction's, which is the owner unless
/// another faction's governorship lists the region (`eur_bavaria`). A building counts only at
/// health 100 or more. Checked against all 238 regions of the shipped start positions and all 72
/// regions of the 9 vanilla saves (`tests/economy_fidelity.rs`, `ECON_RECOMP`). Not modelled
/// (PROVISIONAL, 0 in the shipped files or not in the model): `slots_gdp_values` (all 0 / factor 1.0
/// in the shipped DB), the commodity terms (colonial goods; 0 for every shipped region).
pub fn recompute_region(model: &CampaignModel, region: &Region) -> (u32, i32) {
    recompute_region_with(model, None, region)
}

/// The faction-wide part of an effect a region's GDP / growth reads (`0x00A6AFC0` reads its effect
/// set built by `0x00A67530` = the region's buildings + the governing faction's sum, CONFIRMED): that
/// faction's sum (technologies, faction-wide buildings, ministers, the saved base and the difficulty
/// handicap) without the government part, which [`region_effect`] already counts. It is the
/// **governing** faction's, not the owner's: `eur_bavaria` reads Bavaria's (none) and not Austria's
/// (`tw_growth_technologies` 5, `gdp_mod_all` 3), which is what its stored 1275 / −3 hold.
fn faction_part(model: &CampaignModel, fx: &Effects, region: &Region, engine_key: &str) -> f32 {
    let faction = governing_faction(model, region);
    let gov = model
        .world
        .factions
        .get(&faction)
        .and_then(|f| model.rules.effects.government().get(&f.government_key))
        .map_or(0.0, |s| s.get(engine_key));
    fx.faction(faction, engine_key) - gov
}

/// [`recompute_region`] with the faction-wide effects (`Some`), as the round end runs it. Without
/// them (`None`) it gives the values the shipped start positions store: those were computed before
/// any faction container existed (at a start position `FACTION` #55 equals #54 and techs, scripted
/// bonuses and handicaps have not been summed yet), so they match the building and government
/// effects alone (238 of 238 regions, while 82 regions have a non-zero faction-wide `gdp_mod_all` or
/// `tw_growth_*` that the stored values do not include; `economy_check` ECON_FXGDP).
pub fn recompute_region_with(model: &CampaignModel, fx: Option<&Effects>, region: &Region) -> (u32, i32) {
    let w = region_wealth(model, fx, region, false);
    (w.gdp, w.growth)
}

/// The town wealth growth factors' keys in their slots (`town_wealth_growth_factors` rows; the key
/// objects at `0x015C67F8`, set up by `0x0042ED30`, CONFIRMED).
pub const TOWN_WEALTH_FACTORS: [&str; 10] =
    ["education", "government", "industry", "port", "roads", "tax", "bankruptcy", "technologies", "ministers", "discontent"];

/// A region's GDP and town wealth growth with the growth's breakdown (`0x00A6AFC0`'s outputs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegionWealth {
    /// The GDP.
    pub gdp: u32,
    /// The town wealth growth.
    pub growth: i32,
    /// The growth by [`TOWN_WEALTH_FACTORS`] slot (`0x00A6AFC0`'s fourth output): the faction-wide growth
    /// in industry, each building's education / government / industry / port / roads effect in its slot
    /// (`0x00A6C570`; its home-region effect in none), the technology and home-theatre terms in
    /// technologies and ministers, the tax term (trunc(growth × modifier) when the growth is not negative,
    /// then + the fixed part) in tax, −growth × #17 in bankruptcy and #18 in discontent.
    pub factors: [i32; 10],
}

/// What the region's constructions change in a per-level value when they finish
/// ([`Region::construction_changes`]): Σ value(level built) − value(building replaced).
fn construction_delta(region: &Region, value: impl Fn(&str) -> f32) -> f32 {
    region.construction_changes().map(|(_, new, old)| value(new) - old.map_or(0.0, |b| value(&b.level_key))).sum()
}

/// [`recompute_region_with`] with the growth's breakdown; `predicted` gives the values the region
/// details panel and the wealth trend predict (`0x00A6AFC0` with its last argument 1): the predicted
/// effect set (`0x00A9B930`, [`super::population::predicted_effect_set`]: buildings under construction
/// finished, technologies under research researched) and, per slot, [`Region::predicted_slot_level`].
pub fn region_wealth(model: &CampaignModel, fx: Option<&Effects>, region: &Region, predicted: bool) -> RegionWealth {
    let rules = &model.rules;
    // The predicted set's changes (`0x00A9B930`): the local sets of this region's constructions (DB
    // effect keys, as `region_effect` reads them), and the owner's faction-wide constructions and
    // technologies under research (engine keys, as the faction part reads them).
    let construction_local = |key: &str| construction_delta(region, |level| building_effect(rules, level, key));
    let faction_change = if predicted { super::population::predicted_faction_change(model, region.owner) } else { super::effects::EffectSet::default() };
    let local = |key: &str| region_effect(model, region, key) + if predicted { construction_local(key) } else { 0.0 };
    let extra = |engine: &str| fx.map_or(0.0, |fx| faction_part(model, fx, region, engine)) + faction_change.get(engine);
    let gdp_factor = (local("gdp_mod_all") + extra("gdp_mod_all")) * 0.01 + 1.0;
    let tw_factor = (local("tw_growth_mod_all") + extra("tw_growth_mod_all")) * 0.01 + 1.0;
    let mut gdp = i64::from(region.base_gdp);
    let mut f = [0i32; 10];
    let mut growth: i32 = (local("tw_growth_industry_global") + extra("tw_growth_factionwide")).round_ties_even() as i32;
    f[2] = growth;
    growth = growth.saturating_add(region.discontent_growth);
    f[9] = region.discontent_growth;
    // The building chain's own modifiers (`0x00E1EF10(chain, 1 / 2)`: bonus type 2, `mod_gdp` / `mod_tw_growth`,
    // CONFIRMED reader) from the region's set: its buildings plus the owner's faction sum (techs such as
    // `gdp_mod_farms`).
    let chain_mod = |id: &str, chain: &str| -> f32 {
        let key = |s: &super::effects::EffectSet| s.get_qualified(super::effects::BonusKind::Saved(2), id, chain);
        let mut local: f32 = region.effect_buildings().filter_map(|b| rules.effects.building_local().get(&b.level_key)).map(key).sum();
        if predicted {
            local += construction_delta(region, |level| rules.effects.building_local().get(level).map_or(0.0, key));
        }
        local + fx.map_or(0.0, |fx| fx.faction_qualified(governing_faction(model, region), super::effects::BonusKind::Saved(2), id, chain))
    };
    // The slots (`0x00A638E0`: the slot list, then the road): a building at full health; predicted,
    // [`Region::predicted_slot_level`] (the level being built, nothing for a demolition, the building
    // under repair).
    let slots = (0..region.slots.len()).map(SlotRef::Slot).chain(std::iter::once(SlotRef::Road));
    let levels = slots.filter_map(|s| {
        if predicted {
            region.predicted_slot_level(s)
        } else {
            region.building_at(s).filter(|b| !b.is_damaged()).map(|b| b.level_key.as_str())
        }
    });
    for level in levels {
        let chain = rules.buildings.get(level).map_or("", |x| x.chain.as_str());
        let gdp_f = gdp_factor + chain_mod("1", chain) * 0.01;
        let tw_f = tw_factor + chain_mod("2", chain) * 0.01;
        for key in SLOT_GDP_EFFECTS {
            gdp += (int_effect(rules, level, key) as f32 * gdp_f) as i64;
        }
        for (key, slot) in SLOT_TW_EFFECTS.iter().zip([Some(0), Some(1), None, Some(2), Some(3), Some(4)]) {
            let v = (int_effect(rules, level, key) as f32 * tw_f) as i32;
            growth = growth.saturating_add(v);
            if let Some(i) = slot {
                f[i] = f[i].saturating_add(v);
            }
        }
    }
    let technologies = local("tw_growth_technologies_fixed") + extra("tw_growth_technologies");
    growth = (growth as f32 + technologies) as i32;
    f[7] = technologies as i32;
    // The home-theatre bonus `tw_growth_home_region` (effect 0x36) of the region set once more when the region is in
    // its owner's home theatre (`0x00A8B5C0`; every region of a shipped map is: one theatre each), e.g. the
    // ministers' `tw_growth_ministers_home_theatre`. CONFIRMED reader; the faction part only (the buildings carry
    // none in the shipped data).
    let home = extra("tw_growth_home_region");
    growth = (growth as f32 + home) as i32;
    f[8] = home as i32;
    let modifier = if growth >= 0 { local("tw_growth_taxes_modifier") + extra("tw_growth_tax_modifier") } else { 0.0 };
    let fixed = local("tw_growth_taxes_fixed") + extra("tw_growth_tax_modifier_fixed");
    f[5] = ((growth as f32 * modifier) as i32 as f32 + fixed) as i32;
    f[6] = growth.wrapping_mul(region.wealth_growth_offset).wrapping_neg();
    let scaled = ((modifier - region.wealth_growth_offset as f32) * growth as f32) as i32;
    growth = (scaled.saturating_add(growth) as f32 + fixed) as i32;
    RegionWealth { gdp: gdp.clamp(0, i64::from(u32::MAX)) as u32, growth, factors: f }
}

/// The town wealth trend the region info shows as WealthChange (region +0xC4, set by `0x00AB4410` from
/// the predicted growth, [`region_wealth`] with `predicted`): 0 above +20, 1 above 0, 2 at 0, 3 down to
/// −20, 4 below.
pub fn wealth_trend(predicted_growth: i32) -> u32 {
    match predicted_growth {
        g if g > 20 => 0,
        g if g > 0 => 1,
        0 => 2,
        g if g > -21 => 3,
        _ => 4,
    }
}

/// What [`settle_round`] did with a faction's treasury.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Settlement {
    /// Income and upkeep were applied.
    Paid,
    /// `treasury + income < expenses` (the original's bankruptcy flag, 0x00BBC7D0): the treasury is
    /// emptied. `first_turn` is true on the first such turn in a row (the original shows its
    /// bankruptcy message then, 0x009CBBA0).
    CannotPay {
        /// First bankrupt turn in a row.
        first_turn: bool,
    },
}

/// The round-end economy of one faction (0x00BABE30, then the regions' 0x00AB4410), paying its running
/// record (income = the sum of categories 5..11, expenses = the sum of 18..24): if
/// `treasury + income >= expenses` the treasury changes by `income − expenses` ([`super::treasury::cannot_pay`] /
/// [`super::treasury::settle_round`], without the exe's 32-bit wrap: ORIGINAL BUG, `treasury` module docs). Otherwise the
/// faction is bankrupt ([`Settlement::CannotPay`], CONFIRMED 0x00BABE30 / 0x00BA2030 /
/// 0x008AE710): the treasury becomes 0, the bankrupt-turn count grows, and the capital region's
/// [`Region::wealth_growth_offset`] grows by 2 (at most 6; it falls by 1 at each of the faction's
/// turn ends, which slows that region's town wealth growth). Not modelled (CAMPAIGN_FIDELITY.md
/// §Bankruptcy): the units deserting each bankrupt turn (0x008BA020), the prestige loss and the
/// regular payments cancelled to cover the deficit (0x00B0E580). Then trade routes accumulate
/// value, and every owned region's GDP and town wealth growth are recomputed
/// ([`recompute_region`]) and its town wealth becomes `max(0, town_wealth + town_wealth_growth)`
/// (CONFIRMED).
pub fn settle_round(model: &mut CampaignModel, faction: FactionId) -> Settlement {
    // The faction's effects, once for the whole step: nothing below changes their sources, except
    // desertion, after which they are computed again.
    let mut fx = Effects::compute_for(model, faction);
    // The running record (the history's last, `0x00BABE00`) is what `0x00BABE30` pays: its income
    // 5..11 (`0x00BBE970`) and expenses 18..24 (`0x00BBE910`), plain 32-bit sums, CONFIRMED. Taxes (5)
    // and trade (7) are as last refreshed ([`refresh_running_income`]: a building finished since counts
    // from the next refresh, as in the exe); the expenses are the trackers' ([`set_upkeep_trackers`],
    // [`set_policing_tracker`], set by the round end's first pass, [`CampaignModel::prepare_round_end`]).
    // The other income (11) is booked at the faction's load or creation and by the script's
    // `other_income_mod` ([`script_other_income_mod`]). The treaty money (income 8 / 10, expenses 22 / 24) is booked
    // by its writers ([`book_treaty_money`]). The model keeps no tracker for income 9, so it writes it here:
    // the commerce raids' income (the live raid entries book their takes into group 1 and take them back
    // when dropped, `0x00BBE9A0` / `0x00AC8810`, CONFIRMED).
    let raiding = model.raid_income(faction);
    let (mut revenue, mut expenses) = (0i32, 0i32);
    if let Some(running) = running_record(model, faction) {
        running[9] = raiding;
        revenue = running[5..12].iter().fold(0i32, |a, &v| a.wrapping_add(v));
        expenses = running[18..25].iter().fold(0i32, |a, &v| a.wrapping_add(v));
        // Then the next round's running record goes in at the cursor, the oldest of 10 dropping
        // (`0x00BABE30`).
        let next = next_running_record(running);
        let history = model.world.economy_history.entry(faction).or_default();
        history.push(next);
        if history.len() > super::world::ECONOMY_HISTORY_LEN {
            history.remove(0);
        }
    }
    let mut result = Settlement::Paid;
    if let Some(f) = model.world.factions.get_mut(&faction) {
        if super::treasury::cannot_pay(f.treasury, revenue, expenses) {
            f.treasury = 0;
            let turns = model.world.bankrupt_turns.entry(faction).or_insert(0);
            *turns += 1;
            result = Settlement::CannotPay { first_turn: *turns == 1 };
        } else {
            super::treasury::settle_round(&mut f.treasury, revenue, expenses);
            model.world.bankrupt_turns.remove(&faction);
        }
    }
    if matches!(result, Settlement::CannotPay { .. })
        && let Some(r) = model.world.capital(faction).and_then(|c| model.world.regions.get_mut(&c)).filter(|r| r.owner == faction)
    {
        r.wealth_growth_offset = (r.wealth_growth_offset + 2).min(6);
    }
    // 0x008AE710: bankrupt armies desert from the second bankrupt turn in a row on (gate CONFIRMED, see
    // `bankrupt_desertion`).
    if matches!(result, Settlement::CannotPay { .. }) && model.world.bankrupt_turns.get(&faction).copied().unwrap_or(0) > 1 {
        bankrupt_desertion(model, faction, revenue, expenses);
        fx = Effects::compute_for(model, faction);
    }
    // Trade routes accumulate value (0x00BCB020 -> 0x00B05CC0, after the economy).
    accumulate_trade_with(model, &fx, faction);
    // Each region: GDP and growth recomputed, then the growth applied (0x00AB4410).
    let updates: Vec<(RegionId, u32, i32)> = model
        .world
        .regions
        .values()
        .filter(|r| r.owner == faction)
        .map(|r| {
            let (gdp, growth) = recompute_region_with(model, Some(&fx), r);
            (r.id, gdp, growth)
        })
        .collect();
    for (id, gdp, growth) in updates {
        if let Some(r) = model.world.regions.get_mut(&id) {
            r.gdp = gdp;
            r.town_wealth_growth = growth;
            r.town_wealth = (i64::from(r.town_wealth) + i64::from(growth)).clamp(0, i64::from(u32::MAX)) as u32;
        }
    }
    result
}

/// Public order of a region, per class: happiness + repression, as the original sums them
/// (0x008EDF70 fills the factors of each social class; 0x008E2AB0 = positive + negative happiness +
/// repression). Positive = content.
///
/// Modelled sources:
/// - happiness: the class's tax level (`taxes_effects_jct`), the government type
///   (`government_types_to_effects`) and the region's buildings (`happy_*` effects of the class,
///   `_all` and `_all_classes` ones for both);
/// - repression: `repression_*` effects of the government type and the buildings, the
///   **garrison** ([`garrison_repression`], CONFIRMED 0x008B18F0) and **automated policing**
///   ([`automated_policing`], CONFIRMED 0x008EDF70);
/// - education ([9]: clamour-for-reform effects only in the capital or where research points are,
///   with `education_happy_mod` and the class's stored base), religion ([3], [`religion_factor`]),
///   war results ([11]: the class's stored value, +12 with a hostile character in the region) and
///   gentlemen ([12], [`gentlemen_factor`]) (round 4; see CAMPAIGN_FIDELITY.md §Public order).
///
/// The factors of every class are [`public_order_factors`] (the exe's slots, effects included);
/// `lower` and `upper` are the totals of the government's lower and upper classes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PublicOrder {
    /// Lower classes.
    pub lower: f32,
    /// Upper classes.
    pub upper: f32,
}

impl PublicOrder {
    /// The lower of the two classes (the one that riots first).
    pub fn worst(&self) -> f32 {
        self.lower.min(self.upper)
    }
}


/// The population factor of garrison repression (0x008B18F0, CONFIRMED): small towns are easier to
/// police. `population` is the region's total population.
pub fn garrison_population_factor(population: u32) -> f32 {
    match population {
        0..=9_998 => 1.5,
        9_999..=49_998 => 1.25,
        49_999..=249_998 => 1.0,
        249_999..=999_999 => 0.75,
        1_000_000..=9_999_999 => 0.5,
        _ => 0.3,
    }
}

/// Repression from the garrison (0x008B18F0, CONFIRMED): `round(clamp(strength × population
/// factor, 0, policing_garrison_cap))`. `strength` is the number of units of the settlement's
/// garrison army (0x00B14880 → 0x008B1C30, which counts a unit twice when its record flag +0x136 is
/// set: militia, INFERRED, see `garrison_units`).
pub fn garrison_repression(rules: &CampaignRules, garrison_units: u32, population: u32) -> f32 {
    let cap = rules.var("policing_garrison_cap", 0.0);
    let v = garrison_units as f32 * garrison_population_factor(population);
    let v = if v < 0.0 { 0.0 } else { v.min(cap) };
    v.round_ties_even()
}

/// Automated policing (0x008EDF70, CONFIRMED): when a class's happiness + repression is negative,
/// policing adds `min(|net|, max(0, policing_automated_cap + repression_policing_cap))`.
/// `repression_policing_cap` is a faction effect, not modelled (0).
pub fn automated_policing(rules: &CampaignRules, net: f32) -> f32 {
    if net >= 0.0 {
        return 0.0;
    }
    // The getter result is converted to an integer (0x008B25F0, then integer adds).
    let cap = (rules.var("policing_automated_cap", 0.0) as i32).max(0) as f32;
    (-net).min(cap)
}

/// The number of units garrisoned in a region's settlement.
pub fn garrison_units(model: &CampaignModel, region: &Region) -> u32 {
    // Every force inside the settlement (its garrison and the armies whose commanders are garrisoned
    // there). A unit counts twice when its record flag +0x136 is set: militia (`infantry_militia`;
    // INFERRED class mapping, it reproduces the garrison factor of the vanilla saves).
    model
        .defenders_of(region.id)
        .iter()
        .filter_map(|f| model.world.forces.get(f))
        .flat_map(|f| &f.units)
        .map(|u| if model.rules.units.get(&u.unit_key).is_some_and(|r| r.militia) { 2 } else { 1 })
        .sum()
}

/// One population class's public-order factors, in the exe's slots (`0x008EDF70`, CONFIRMED layout;
/// saved as `POPULATION_CLASS` #1 i32[13] and #2 i32[6]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassPublicOrder {
    /// The class (`lower`, `middle`, `upper`).
    pub class: String,
    /// Happiness [1..13]: 0 government type, 1 taxes, 2 religion, 3 events, 4 culture, 5 industry,
    /// 6 characters / ministers / traits, 7 (unused), 8 education, 9 (unused), 10 war results,
    /// 11 gentlemen, 12 religion with [`super::features::CampaignFeatures::alignment_public_order`] (`spa_napoleon`).
    pub happiness: [i32; 13],
    /// Repression [16..21]: 0 government type, 1 government buildings, 2 ministers, 3 automated
    /// policing, 4 garrison, 5 (set elsewhere: 0).
    pub repression: [i32; 6],
}

impl ClassPublicOrder {
    /// The class's public order (`0x008E2AB0`): positive + negative happiness + repression.
    pub fn total(&self) -> i32 {
        self.happiness.iter().sum::<i32>() + self.repression.iter().sum::<i32>()
    }
}

/// The region's effect set as `0x008EDF70` reads it (`0x00A67530`): its buildings (at full health),
/// the owner's faction sum and the tax-level bundles of both classes (compiled through the effect
/// mapping).
pub fn region_effect_set(model: &CampaignModel, reg: &Region) -> super::effects::EffectSet {
    let rules = &model.rules;
    // The faction part comes from the governing faction (normally the owner).
    let gov = model.world.governing_faction(reg.id).unwrap_or(reg.owner);
    let mut set = super::effects::Effects::faction_sum(model, gov);
    for b in reg.effect_buildings() {
        if let Some(s) = rules.effects.building_local().get(&b.level_key) {
            set.merge(s);
        }
    }
    // A tax-exempt region has no tax effects (vanilla saves: the tax factor is 0 there). The bundles are
    // compiled once, when set, through the rules' fixed mapping (`EffectRules::set_tax_bundle`).
    if let Some(f) = model.world.factions.get(&gov).filter(|_| !reg.tax_exempt) {
        for (class, level) in [(TaxClass::Lower, &f.tax_lower), (TaxClass::Upper, &f.tax_upper)] {
            if let Some(s) = rules.effects.tax_set(class.key(), level) {
                set.merge(s);
            }
        }
    }
    set
}

/// The public-order factors of every population class of a region (`0x008EDF70`, CONFIRMED
/// sources and slots). Classes are the region's `POPULATION CLASSES` (lower and upper when none
/// were loaded: the government's two). The government type names a lower and an upper class
/// (`government_types` #5 / #4, CONFIRMED: a constitutional monarchy governs `upper` and `middle`, a
/// republic and an empire `middle` and `lower`): its lower class takes the lower government-type and tax
/// happiness (effects 9 / 10), its upper class the upper ones (11 / 12), and any other class gets no
/// factors (all 0, as the vanilla saves store them).
pub fn public_order_factors(model: &CampaignModel, region: RegionId) -> Vec<ClassPublicOrder> {
    let Some(reg) = model.world.regions.get(&region) else { return Vec::new() };
    public_order_factors_with(model, reg, &region_effect_set(model, reg), &reg.religions, garrison_units(model, reg))
}

/// [`public_order_factors`] from the inputs `0x008EDF70` takes: the effect set (the region's, or the
/// predicted one for the panel's projection), the religion breakdown the religion factor reads (the
/// population object's, a projected copy's for the panel) and the garrison's unit count (`0x008B18F0`'s
/// argument; −1 there means the garrison's own). The garrison factor's population is always the
/// region's own (`0x008B18F0` reads region +0x28, not the copy).
pub fn public_order_factors_with(model: &CampaignModel, reg: &Region, set: &super::effects::EffectSet, religions: &[(String, f32)], garrison_units: u32) -> Vec<ClassPublicOrder> {
    use super::effects::BonusKind;
    let region = reg.id;
    if !model.world.factions.contains_key(&reg.owner) {
        return Vec::new();
    }
    let rules = &model.rules;
    let (upper_class, lower_class) = government_classes(model, model.world.governing_faction(region).unwrap_or(reg.owner));
    let int = |b: &str| set.get_int(b);
    let class_int = |b: &str, class: &str| f64::from(set.get_qualified(BonusKind::PopClass, b, class)).round_ties_even() as i32;
    let alignment = rules.features.alignment_public_order;
    let schools = model.world.capital(reg.owner) == Some(region) || int("research_points") != 0;
    let state = model.world.faction_details.get(&reg.owner).map(|d| d.religion.clone()).unwrap_or_default();
    // The owner's (faction-level) conversion only: the region's own buildings do not count here (the vanilla spa
    // saves: churches with `conversion_anti_french` leave the stored factor unchanged).
    let local: f32 = reg.effect_buildings().filter_map(|b| rules.effects.building_local().get(&b.level_key)).map(|s| s.get_qualified(BonusKind::Religion, "conversion", &state)).sum();
    let conversion = set.get_qualified(BonusKind::Religion, "conversion", &state) - local;
    let religion = religion_factor(model, reg, religions, int("happiness_mod_religious_unrest"), conversion) as i32;
    let gentlemen = gentlemen_factor(model, reg) as i32;
    let hostile = !alignment && hostile_in_region(model, reg);
    let garrison = garrison_repression(rules, garrison_units, reg.population) as i32;
    let classes: Vec<(String, i32, i32)> =
        if reg.class_bases.is_empty() { vec![(lower_class.clone(), 0, 0), (upper_class.clone(), 0, 0)] } else { reg.class_bases.clone() };
    classes
        .into_iter()
        .map(|(class, edu_base, war_base)| {
            let side = if class == lower_class {
                "lower"
            } else if class == upper_class {
                "upper"
            } else {
                return ClassPublicOrder { class, ..Default::default() };
            };
            let mut h = [0i32; 13];
            h[0] = int(&format!("happiness_active_{side}_gov_type"));
            h[1] = int(&format!("happiness_active_{side}_tax"));
            if !alignment {
                h[2] = religion;
            } else {
                h[12] = religion;
            }
            h[3] = int("happiness_events_regional") + int("happiness_events_factional");
            h[4] = class_int("happiness_culture", &class);
            h[5] = class_int("happiness_industry", &class);
            h[6] = class_int("happiness_character", &class) + int("happiness_ministerial_position") + int("happiness_character_trait_or_ancillary");
            if schools {
                let clamour = set.get_qualified(BonusKind::PopClass, "happiness_clamour_for_reform", &class);
                h[8] = edu_base + ((set.get("education_happy_mod") * 0.01 + 1.0) * clamour).round_ties_even() as i32;
            }
            h[10] = war_base + if war_base == 0 && hostile { 12 } else { 0 };
            h[11] = gentlemen;
            let mut r = [0i32; 6];
            r[0] = int("repression_gov_type");
            r[1] = int("repression_gov_building");
            r[2] = int("repression_ministers");
            r[4] = garrison;
            let net = h.iter().sum::<i32>() + r.iter().sum::<i32>();
            if net < 0 {
                let cap = (rules.var("policing_automated_cap", 0.0) as i32 + int("repression_policing_cap")).max(0);
                r[3] = (-net).min(cap);
            }
            ClassPublicOrder { class, happiness: h, repression: r }
        })
        .collect()
}

/// See [`PublicOrder`]: the totals of the government's lower and upper classes (see
/// [`public_order_factors`]).
pub fn public_order(model: &CampaignModel, region: RegionId) -> PublicOrder {
    let (upper, lower) = governed_class_factors(model, region);
    let total = |c: Option<ClassPublicOrder>| c.map_or(0.0, |f| f.total() as f32);
    PublicOrder { lower: total(lower), upper: total(upper) }
}

/// The [`public_order_factors`] of the region's government's upper and lower classes (see
/// [`government_classes`]; the governing faction's government), in that order; `None` for a class
/// the region does not have.
pub fn governed_class_factors(model: &CampaignModel, region: RegionId) -> (Option<ClassPublicOrder>, Option<ClassPublicOrder>) {
    let Some(reg) = model.world.regions.get(&region) else { return (None, None) };
    governed_classes(model, reg, public_order_factors(model, region))
}

/// The government's upper and lower classes out of `factors` (a region's [`public_order_factors`] or
/// [`public_order_factors_with`]), as [`governed_class_factors`] picks them.
pub fn governed_classes(model: &CampaignModel, reg: &Region, mut factors: Vec<ClassPublicOrder>) -> (Option<ClassPublicOrder>, Option<ClassPublicOrder>) {
    let (upper, lower) = government_classes(model, model.world.governing_faction(reg.id).unwrap_or(reg.owner));
    let mut take = |c: &str| factors.iter().position(|f| f.class == c).map(|i| factors.swap_remove(i));
    let u = take(&upper);
    let l = if lower == upper { u.clone() } else { take(&lower) };
    (u, l)
}

/// The (upper, lower) population classes of a faction's government type (`government_types` #4 / #5;
/// `upper` and `lower` when the type is not in the rules).
pub fn government_classes(model: &CampaignModel, faction: FactionId) -> (String, String) {
    model
        .world
        .factions
        .get(&faction)
        .and_then(|f| model.rules.government_classes.get(&f.government_key))
        .cloned()
        .unwrap_or_else(|| ("upper".into(), "lower".into()))
}

/// Distance from the settlement within which a character counts as standing in the region
/// when the campaign path grid is not loaded (PROVISIONAL fallback: the original queries the region's
/// area, 0x00BA6840, which the model reads from the path grid when terrain is loaded).
pub const REGION_AREA_RADIUS: f32 = 20.0;

/// `0x00AA10F0`: a character of a faction at war with the region's owner stands in the region.
fn hostile_in_region(model: &CampaignModel, reg: &Region) -> bool {
    // Only characters with an army count (0x009D3A60; navies do not: the vanilla eur save has a
    // British fleet off Rotterdam and Holland shows no war-results factor).
    model.world.forces.values().filter(|f| !f.is_navy && f.faction != reg.owner && model.world.stance(reg.owner, f.faction) == super::Stance::War).any(|f| {
        f.commander.and_then(|c| model.world.characters.get(&c)).is_some_and(|c| in_region(model, reg, c))
    })
}

/// The religion factor of public order (`0x008B1B10`, CONFIRMED): `−round(max(0, 100 + unrest) / 100 ×
/// (Σ share × 100 × value − conversion))` over the region's religions, with `value` =
/// `diplomatic_relations_religion` #3 for (region religion, state religion) (0 for a missing pair),
/// `unrest` = the region set's `happiness_mod_religious_unrest` and `conversion` = the owner's
/// `conversion` effect for its religion (bonus type 10, the religion-keyed one; type 7 is population class × religion). Reproduces the religion factor of
/// every population class in the vanilla eur save `auto_nr4_t4` (CAMPAIGN_FIDELITY.md §Public order).
/// `religions` is the breakdown it reads (the region's, or the panel's projected copy).
pub fn religion_factor(model: &CampaignModel, reg: &Region, religions: &[(String, f32)], unrest: i32, conversion: f32) -> f32 {
    let Some(state) = model.world.faction_details.get(&reg.owner).map(|d| d.religion.as_str()).filter(|s| !s.is_empty()) else {
        return 0.0;
    };
    let sum: f32 = religions
        .iter()
        .map(|(r, share)| share * 100.0 * model.rules.religion_relations.get(&(r.clone(), state.to_string())).copied().unwrap_or(0.0))
        .sum();
    let m = unrest as f32 + 100.0;
    if m < 0.0 { 0.0 } else { -(m * 0.01 * (sum - conversion)).round_ties_even() }
}


/// The smallest strength a unit may keep (`0x008F68F0`, CONFIRMED): `ceil(unit_minimum_strength ×
/// max men)`, at least 4, at most the max men.
pub fn unit_minimum_men(model: &CampaignModel, max_men: u32) -> u32 {
    if max_men == 0 {
        return 0;
    }
    let v = (model.rules.var("unit_minimum_strength", 0.0) * max_men as f32).ceil() as u32;
    if v < 4 { 4 } else { v.min(max_men) }
}

/// The unit classes whose units never desert (`0x008BA1E0`, CONFIRMED): it skips the units whose `UNIT_RECORD`
/// +0x20 is 0xE, 0xB, 4 or 0xC. The record constructor `0x00E91320` sets +0x20 from the `units` #3 class key
/// through the class enum `0x00EED3E0` (a fixed list of keys; any other key is 0), whose codes 4 / 0xB / 0xC / 0xE
/// are these four keys.
pub const DESERTION_EXEMPT_CLASSES: [&str; 4] = ["cavalry_heavy", "elephants", "general", "infantry_elite"];

/// Bankrupt armies desert (`0x008BA020` / `0x008BA1E0`, CONFIRMED formula): every unit of a force
/// outside a settlement, except the commander's own unit, loses `round(r × men)` men with
/// `r = 0.07 + rand × (p − 0.07)` (one campaign-RNG step per unit) and
/// `p = 0.3` if the income is below 1, else `clamp((expenses / income − 1) × 0.3 + 0.07, 0.07, 0.3)`;
/// a unit left at or under [`unit_minimum_men`] is disbanded. It runs while faction `+0x50C` > 1: that field is
/// the bankrupt-turn count (the economics object sits at faction +0xAC, and its +0x460 is the counter; CONFIRMED
/// with a debugger write-watch, CAMPAIGN_FIDELITY.md §Bankruptcy). Units of the [`DESERTION_EXEMPT_CLASSES`]
/// are skipped too, and draw no random number.
pub fn bankrupt_desertion(model: &mut CampaignModel, faction: FactionId, income: i32, expenses: i32) {
    let p = if income < 1 { 0.3f32 } else { ((expenses as f32 / income as f32 - 1.0) * 0.3 + 0.07).clamp(0.07, 0.3) };
    let garrisons: Vec<ForceId> = model.world.regions.values().filter_map(|r| r.garrison).collect();
    let forces: Vec<ForceId> = model
        .world
        .forces
        .values()
        .filter(|f| f.faction == faction && f.commander.is_some() && !garrisons.contains(&f.id))
        .filter(|f| f.commander.and_then(|c| model.world.characters.get(&c)).is_some_and(|c| c.garrisoned_in.is_none()))
        .map(|f| f.id)
        .collect();
    let mut emptied = Vec::new();
    for fid in forces {
        let Some(f) = model.world.forces.get(&fid) else { continue };
        let commander = f.commander;
        let minimums: Vec<u32> = f.units.iter().map(|u| unit_minimum_men(model, u.max_men)).collect();
        let mut gone = Vec::new();
        let f = model.world.forces.get_mut(&fid).expect("checked above");
        for (i, u) in f.units.iter_mut().enumerate() {
            let exempt = model.rules.units.get(&u.unit_key).is_some_and(|r| DESERTION_EXEMPT_CLASSES.contains(&r.unit_class.as_str()));
            if exempt || (commander.is_some() && u.character == commander) {
                continue;
            }
            let r = model.rng.unit_float() * (p - 0.07) + 0.07;
            let lost = (r * u.men as f32).round_ties_even() as u32;
            u.men = u.men.saturating_sub(lost);
            if u.men <= minimums[i] {
                gone.push(i);
            }
        }
        for i in gone.into_iter().rev() {
            f.units.remove(i);
        }
        if f.units.is_empty() {
            emptied.push(fid);
        }
    }
    let mut events = Vec::new();
    for fid in emptied {
        model.destroy_force(fid, &mut events);
    }
}


/// `true` if the character stands in the region: garrisoned there, else the path grid's region at his
/// position, else (no terrain loaded) within [`REGION_AREA_RADIUS`] of its settlement.
pub fn in_region(model: &CampaignModel, reg: &Region, c: &super::Character) -> bool {
    if c.garrisoned_in == Some(reg.id) {
        return true;
    }
    // With the map: the region under the character (the path grid's region cells).
    if let Some(t) = &model.terrain {
        return t.0.region_at(c.position.0.to_f32(), c.position.1.to_f32()) == Some(reg.key.as_str());
    }
    let (sx, sy) = (reg.settlement.position.0.to_f32(), reg.settlement.position.1.to_f32());
    c.garrisoned_in.is_none() && (c.position.0.to_f32() - sx).powi(2) + (c.position.1.to_f32() - sy).powi(2) <= REGION_AREA_RADIUS * REGION_AREA_RADIUS
}

/// The gentlemen factor of public order (`0x008C55D0`, CONFIRMED): each gentleman in the region gives
/// `ceil(x / gentleman_happiness_divisor) + gentleman_happiness_bonus` — positive for the owner's
/// own gentlemen, negative for others'; the positive sum is clamped to
/// [0, `gentleman_happiness_positive_limit`], the negative one floored at
/// `gentleman_happiness_negative_limit`. x is the gentleman's skill (`0x00A198D0`): the model uses his
/// `research` attribute level (INFERRED; the effects on it are not modelled) and the bonus effect 0.
pub fn gentlemen_factor(model: &CampaignModel, reg: &Region) -> f32 {
    let rules = &model.rules;
    let divisor = (rules.var("gentleman_happiness_divisor", 1.0) as i32).max(1);
    let (mut pos, mut neg) = (0i32, 0i32);
    for c in model.world.characters.values().filter(|c| c.kind == super::CharacterKind::Gentleman && in_region(model, reg, c)) {
        // The skill 0x00A198D0 has no term for a gentleman beyond effects: 0 (vanilla saves: own
        // gentlemen in their capitals add nothing). The bonus is the character's + faction's effect 129.
        let x = 0i32;
        let bonus = super::effects::Effects::faction_sum(model, c.faction).get_int("gentleman_happiness_bonus");
        let v = (x + divisor - 1) / divisor + bonus;
        if c.faction == reg.owner { pos += v } else { neg -= v }
    }
    let pos = pos.clamp(0, rules.var("gentleman_happiness_positive_limit", 0.0) as i32);
    let floor = rules.var("gentleman_happiness_negative_limit", 0.0) as i32;
    (if neg < floor { floor } else { neg.min(0) } + pos) as f32
}
