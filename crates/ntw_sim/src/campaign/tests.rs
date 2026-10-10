//! Campaign model tests on a small MADE-UP world (no game data).

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use super::effects::{BonusKind, EffectKey, Effects, SavedBonus, TECH_RESEARCHED};
use crate::calendar::{Calendar, Date, HALF_EARLY, HALF_LATE};
use crate::fixed::Fixed20;
use crate::rng::CaRng;

const A: FactionId = FactionId(1);
const B: FactionId = FactionId(2);
const C: FactionId = FactionId(3);

/// `rows`, after asserting that the test mapping ([`CampaignRules::test_rules_with`]) maps every effect
/// key in them: an unmapped key compiles to nothing (as an unmapped DB key does), so a test inserting
/// one would silently check nothing. Every effect set a test inserts goes through it.
fn mapped<K: AsRef<str>>(rules: &CampaignRules, rows: Vec<(K, f32)>) -> Vec<(K, f32)> {
    for (k, _) in &rows {
        assert!(rules.effects.maps(k.as_ref()), "effect key {:?} is not in the test mapping (CampaignRules::test_rules_with)", k.as_ref());
    }
    rows
}

/// One test mapping row: DB effect key `db` → the basic bonus `bonus`.
fn map_basic(db: &str, bonus: &str) -> (String, Vec<EffectKey>) {
    (db.into(), vec![EffectKey::basic(bonus)])
}

/// One test mapping row: DB effect key `db` → the chain cost bonus (saved kind 2) of `chain`.
fn map_chain_cost(db: &str, chain: &str) -> (String, Vec<EffectKey>) {
    (db.into(), vec![EffectKey { kind: BonusKind::Saved(2), bonus: "0".into(), qualifier: chain.into() }])
}

/// The tax bundles' growth terms of [`tax_normal_growth_bundles`], under the engine names the growth
/// reads (`recompute_region_with`).
fn tax_growth_mapping() -> [(String, Vec<EffectKey>); 2] {
    [map_basic("tw_growth_taxes_modifier", "tw_growth_tax_modifier"), map_basic("tw_growth_taxes_fixed", "tw_growth_tax_modifier_fixed")]
}

/// [`test_model`] on [`CampaignRules::test_rules_with`] `extra`: the effect keys this test compiles.
fn test_model_with(extra: impl IntoIterator<Item = (String, Vec<EffectKey>)>) -> CampaignModel {
    let mut m = test_model();
    m.rules = Arc::new(CampaignRules::test_rules_with(extra));
    m
}

fn pos(x: i32, z: i32) -> (Fixed20, Fixed20) {
    (Fixed20::from_int(x), Fixed20::from_int(z))
}

fn faction(id: FactionId, key: &str, treasury: i32, government: GovernmentType) -> Faction {
    Faction {
        id,
        key: key.into(),
        treasury,
        government,
        government_key: String::new(),
        tax_lower: "tax_normal".into(),
        tax_upper: "tax_normal".into(),
        diplomacy: BTreeMap::new(),
    }
}

fn region(id: u32, owner: FactionId, gdp: u32, slots: usize) -> Region {
    let mut s: Vec<RegionSlot> = (0..slots)
        .map(|i| RegionSlot { key: format!("test_slot_{i}"), slot_type: "test_slot".into(), building: None, position: None, port: false, holder: None, id: 0 })
        .collect();
    if slots > 0 {
        s[0].building = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    }
    Region {
        id: RegionId(id),
        key: format!("test_region_{id}"),
        owner,
        settlement: Settlement { key: format!("settlement:test_region_{id}:town"), position: pos(id as i32, 0) },
        slots: s,
        road: None,
        fortification: None,
        population: gdp * 100,
        base_gdp: gdp,
        gdp,
        wealth_growth_offset: 0,
        discontent_growth: 0,
        town_wealth: 0,
        town_wealth_growth: 0,
        tax_exempt: false,
        religions: Vec::new(),
        class_bases: Vec::new(),
        // The shipped start positions give every region a base capacity of twice its population.
        population_state: super::population::PopulationState { base_capacity: gdp * 200, trend: 2, ..Default::default() },
        recruitment_queue: Vec::new(),
        construction: Vec::new(),
        garrison: None,
        fleet: None,
    }
}

/// The id of the `i`-th item of `region`'s recruitment queue.
fn queued(m: &CampaignModel, region: RegionId, i: usize) -> super::RecruitmentItemId {
    m.world.regions[&region].recruitment_queue[i].id
}

fn character(id: i32, faction: FactionId, kind: CharacterKind, mp: i32) -> Character {
    Character {
        id: CharacterId(id),
        faction,
        kind,
        position: pos(0, 0),
        movement_points: mp,
        max_movement_points: mp,
        base_movement_points: mp,
        garrisoned_in: None,
    }
}

fn unit(id: i32) -> CampaignUnit {
    CampaignUnit { id: UnitId(id), unit_key: "test_unit".into(), men: 80, max_men: 100, character: None, officer_name: Default::default() }
}

/// A small MADE-UP world: 3 factions (turn order B, A, C), 3 regions, 3 characters, 2 forces,
/// with the made-up [`CampaignRules::test_rules`].
pub(super) fn test_model() -> CampaignModel {
    let mut w = World::default();
    for f in [
        faction(A, "test_faction_a", 1000, GovernmentType::AbsoluteMonarchy),
        faction(B, "test_faction_b", 1000, GovernmentType::Republic),
        faction(C, "test_faction_c", 0, GovernmentType::ConstitutionalMonarchy),
    ] {
        w.factions.insert(f.id, f);
    }
    // Turn order B, A, C: deliberately not id order.
    w.turn_order = vec![B, A, C];
    for r in [region(10, A, 4000, 2), region(11, B, 8000, 1), region(12, A, 2000, 0)] {
        w.regions.insert(r.id, r);
    }
    for c in [
        character(100, A, CharacterKind::General, 30),
        character(101, B, CharacterKind::Colonel, 25),
        character(102, A, CharacterKind::Minister, 0),
    ] {
        w.characters.insert(c.id, c);
    }
    w.forces.insert(
        ForceId(1000),
        MilitaryForce { id: ForceId(1000), faction: A, commander: Some(CharacterId(100)), units: vec![unit(1)], is_navy: false },
    );
    w.forces.insert(
        ForceId(1001),
        MilitaryForce { id: ForceId(1001), faction: B, commander: Some(CharacterId(101)), units: vec![unit(2), unit(3)], is_navy: false },
    );
    let start = Date { year: 1805, season: 1, month: 0, half: HALF_EARLY };
    let mut m = CampaignModel::new(Calendar::new(start, 0), CaRng::new(12345), w);
    m.rules = Arc::new(CampaignRules::test_rules());
    m
}

#[test]
fn end_turn_advances_calendar() {
    let mut m = test_model();
    m.end_turn();
    assert_eq!(m.calendar.date.half, HALF_LATE);
    assert_eq!(m.calendar.date.month, 0);
    assert_eq!(m.calendar.turns_elapsed, 1);
    assert_eq!(m.calendar.turn_number(), 2);
    m.apply(CampaignCommand::EndTurn).unwrap();
    assert_eq!(m.calendar.date.month, 1);
    assert_eq!(m.calendar.date.half, HALF_EARLY);
}

#[test]
fn twenty_four_turns_is_one_year() {
    let mut m = test_model();
    let start = m.calendar.date;
    for _ in 0..24 {
        m.end_turn();
    }
    assert_eq!(m.calendar.date.year, start.year + 1);
    assert_eq!(m.calendar.date.month, start.month);
    assert_eq!(m.calendar.date.half, start.half);
    assert_eq!(m.calendar.turns_elapsed, 24);
    assert_eq!(m.calendar.turn_in_year, 0);
}

#[test]
fn income_and_upkeep() {
    // MADE-UP rules: tax_normal 15 %, faction_gdp_other 100, upkeep 10 per test_unit, no
    // tax_efficiency variables (no penalty). Both classes pay 15 % of GDP + town wealth.
    // A: regions 4000 and 2000: 2 × 600 + 2 × 300 = 1800, other 100, upkeep 10 -> +1890.
    // B: 8000: 2 × 1200 = 2400 + 100 - 20 -> +2480.  C: no regions, +100.
    let m = test_model();
    let ia = economy::faction_income(&m, A);
    assert_eq!((ia.taxes, ia.other, ia.upkeep, ia.net()), (1800, 100, 10, 1890));
    assert_eq!(economy::faction_income(&m, B).net(), 2480);
    assert_eq!(economy::faction_income(&m, C).net(), 100);
    assert_eq!(economy::faction_income(&m, FactionId(99)).net(), 0);

    // No humans: the first end_turn plays round 1; the economy of every faction is settled once,
    // at the round end.
    let mut m = test_model();
    m.end_turn();
    assert_eq!(m.world.factions[&B].treasury, 1000 + 2480);
    assert_eq!(m.world.factions[&A].treasury, 1000 + 1890);
    assert_eq!(m.world.factions[&C].treasury, 100);

    // A higher tax level raises income: 6000 × 20 % × 2 = 2400.
    let mut m = test_model();
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Lower, level: "tax_high".into() }).unwrap();
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Upper, level: "tax_high".into() }).unwrap();
    assert_eq!(economy::faction_income(&m, A).taxes, 2400);
    // The tax level makes the lower classes unhappier (made-up effect -10).
    assert_eq!(economy::public_order(&m, RegionId(12)).lower, -10.0);
    // Buildings add happiness (made-up +1 "happy_culture_all" on test_building_level).
    assert_eq!(economy::public_order(&m, RegionId(10)).upper, 1.0);
}

#[test]
fn economy_is_settled_at_round_end() {
    // A is human: starting the campaign plays B's turn and starts A's; nobody is paid yet.
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    assert_eq!(m.world.factions[&A].treasury, 1000);
    assert_eq!(m.world.factions[&B].treasury, 1000);
    // A ends the turn: C plays, the round ends and every faction is paid once, then B plays.
    m.end_turn();
    assert_eq!(m.world.factions[&A].treasury, 1000 + 1890);
    assert_eq!(m.world.factions[&B].treasury, 1000 + 2480);
    assert_eq!(m.world.factions[&C].treasury, 100);
}

#[test]
fn tax_efficiency_matches_the_original() {
    // The shipped campaign_variables values (tax_efficiency_*: minimum 4, log base 1.25,
    // modifier -4.5, total regions 136); formula of 0x00BC73C0.
    let mut rules = CampaignRules::test_rules();
    assert_eq!(economy::tax_efficiency(&rules, 12, 0), 0.0, "no variables, no penalty");
    for (k, v) in [
        ("tax_efficiency_regions_minimum", 4.0),
        ("tax_efficiency_log_base", 1.25),
        ("tax_efficiency_modifier", -4.5),
        ("tax_efficiency_total_regions", 136.0),
    ] {
        rules.variables.insert(k.into(), v);
    }
    assert_eq!(economy::tax_efficiency(&rules, 1, 0), 0.0);
    assert_eq!(economy::tax_efficiency(&rules, 4, 0), 0.0);
    // 12 regions: sqrt(ln(1 + 8/136) / ln 1.25) × -4.5 × 0.1 = -0.2278.
    let e12 = economy::tax_efficiency(&rules, 12, 0);
    assert!((e12 + 0.2278).abs() < 1e-4, "{e12}");
    // admin_cost_mod adds to the region count.
    assert_eq!(economy::tax_efficiency(&rules, 10, 2), e12);
    assert!(economy::tax_efficiency(&rules, 40, 0) < e12);

    // Effective rate (0x00BA4210): (1 + e) × (rate/100 + half of each bonus/100), never below 0.
    let r = economy::effective_tax_rate(15, 0.0, economy::TaxBonuses { building: 10.0, ..Default::default() });
    assert!((r - 0.2).abs() < 1e-6, "{r}");
    assert_eq!(economy::effective_tax_rate(0, -2.0, economy::TaxBonuses::default()), 0.0);
    // Class taxes round to nearest, ties to even (FISTP): 0.26 × 2625 = 682.5 -> 682.
    assert_eq!(economy::class_taxes(0.26, 1875, 750), 682);
    assert_eq!(economy::class_taxes(0.26, 1460, 900), 614);
}

/// Each round end adds the turn's record to the economics history (`0x00BABE30`, also when the
/// faction cannot pay), at most 10 kept; the world's net sums the factions' last income minus
/// expenses, without the rebel faction (`0x0096D2E0`).
#[test]
fn the_round_end_records_the_economy_history() {
    let mut m = test_model();
    let income = economy::faction_income(&m, A);
    for _ in 0..12 {
        economy::settle_round(&mut m, A);
    }
    let history = &m.world.economy_history[&A];
    assert_eq!(history.len(), super::world::ECONOMY_HISTORY_LEN);
    let last = history.last().unwrap();
    assert_eq!((last[5], last[7], last[11]), (income.taxes, income.trade, income.other));
    assert_eq!(last[19] + last[20], income.upkeep);
    assert_eq!(m.world.last_income(A), income.revenue());
    assert_eq!(m.world.last_expenses(A), income.upkeep);
    // Only A has a history: the world's net is A's.
    assert_eq!(m.world_net_income(), income.revenue() - income.upkeep);
    // A rebel faction's record does not count.
    let rebel = m.world.factions.keys().copied().find(|&f| m.is_rebel_faction(f));
    if let Some(r) = rebel {
        m.world.economy_history.insert(r, vec![[1000; 25]]);
        assert_eq!(m.world_net_income(), income.revenue() - income.upkeep);
    }
}

#[test]
fn exemption_town_wealth_and_bankruptcy() {
    let mut m = test_model();
    // A tax-exempt region pays nothing.
    m.world.regions.get_mut(&RegionId(12)).unwrap().tax_exempt = true;
    assert_eq!(economy::faction_income(&m, A).taxes, 1200);
    // Town wealth is taxed too and grows at the round end by the recomputed growth (made-up
    // tw_growth_industry 25 on the region's building), never below 0.
    let mut rules = (*m.rules).clone();
    rules.buildings.get_mut("test_building_level").unwrap().effects.push(("tw_growth_industry".into(), 25.0));
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(10)).unwrap().town_wealth = 1000;
    assert_eq!(economy::faction_income(&m, A).taxes, 2 * 750);
    assert_eq!(economy::settle_round(&mut m, A), economy::Settlement::Paid);
    assert_eq!(m.world.regions[&RegionId(10)].town_wealth_growth, 25);
    assert_eq!(m.world.regions[&RegionId(10)].town_wealth, 1025);
    m.world.regions.get_mut(&RegionId(10)).unwrap().discontent_growth = -5000;
    economy::settle_round(&mut m, A);
    assert_eq!(m.world.regions[&RegionId(10)].town_wealth, 0);

    // Cannot pay (treasury + income < upkeep).
    let mut m = test_model();
    {
        let r = m.world.regions.get_mut(&RegionId(11)).unwrap();
        r.gdp = 0;
        r.base_gdp = 0;
    }
    m.world.factions.get_mut(&B).unwrap().treasury = -150;
    // B: income 100, upkeep 20: -150 + 100 < 20.
    // Bankrupt: the treasury is emptied and the capital's growth offset grows by 2 (at most 6).
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    assert_eq!(economy::settle_round(&mut m, B), economy::Settlement::CannotPay { first_turn: true });
    assert_eq!(m.world.factions[&B].treasury, 0);
    assert_eq!(m.world.regions[&RegionId(11)].wealth_growth_offset, 2);
    m.world.factions.get_mut(&B).unwrap().treasury = -150;
    assert_eq!(economy::settle_round(&mut m, B), economy::Settlement::CannotPay { first_turn: false });
    assert_eq!(m.world.bankrupt_turns[&B], 2);
    // From the second bankrupt turn, B's army outside a settlement deserts. Income 100 and upkeep 20 give
    // p = clamp((20 / 100 - 1) × 0.3 + 0.07, 0.07, 0.3) = 0.07, so every unit loses round(0.07 × 80) = 6.
    assert!(m.world.forces[&ForceId(1001)].units.iter().all(|u| u.men == 74));
    // A unit at or under the minimum strength (ceil(0.05 × 100) = 5) is disbanded.
    assert_eq!(economy::unit_minimum_men(&m, 100), 5);
    assert_eq!(economy::unit_minimum_men(&m, 20), 4);
    for _ in 0..3 {
        m.world.factions.get_mut(&B).unwrap().treasury = -150;
        economy::settle_round(&mut m, B);
    }
    assert_eq!(m.world.regions[&RegionId(11)].wealth_growth_offset, 6);
    m.world.factions.get_mut(&B).unwrap().treasury = -80;
    assert_eq!(economy::settle_round(&mut m, B), economy::Settlement::Paid);
    assert_eq!(m.world.factions[&B].treasury, 0);
    assert!(!m.world.bankrupt_turns.contains_key(&B));
    // The offset slows the capital's town wealth growth: (modifier - offset) × growth.
    let mut r = m.world.regions[&RegionId(11)].clone();
    r.discontent_growth = 10;
    assert_eq!(economy::recompute_region(&m, &r).1, 10 - 6 * 10);
}

/// The three region-growth misses 0-B round 14 closed, one per region, with the `tax_normal`
/// bundles of the shipped DB (`tw_growth_taxes_modifier` -0.55 over both classes,
/// `tw_growth_taxes_fixed` -5 over both classes) and the growth terms the vanilla saves carry.
///
/// * `eur_moravia` (Austria, tax-exempt): the technology term survives, the tax term does not
///   (5 stored, -2 before the fix).
/// * `eur_wallachia` (the Ottomans, tax-exempt, no term at all): 0 stays 0 (it was -5).
/// * `eur_bavaria` (Austria owns it, the landless Bavaria governs it): the buildings in its slots
///   count, the faction-wide part is Bavaria's (nothing), not Austria's, so the raw 4 (roads only)
///   is taxed to -3 and the GDP carries no `gdp_mod_all` factor.
fn tax_normal_growth_bundles(rules: &mut CampaignRules) {
    for class in ["lower_classes", "upper_classes"] {
        rules.effects.set_tax_bundle(
            class,
            "tax_normal",
            mapped(rules, vec![("tw_growth_taxes_modifier".into(), -0.275), ("tw_growth_taxes_fixed".into(), -2.5)]),
        );
    }
}

/// One made-up researched technology of `effects` on A's researched list (keys the test mapping maps to
/// themselves, as `effect_bonus_value_basic_junction` does in the shipped DB).
fn with_technology(m: &mut CampaignModel, effects: &[(&str, f32)]) {
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_growth_tech".into(), mapped(&rules, effects.to_vec()));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies.push(("test_growth_tech".into(), TECH_RESEARCHED));
}

#[test]
fn tax_exempt_region_keeps_its_technology_growth_without_the_tax_term() {
    // eur_moravia: an iron mine and a manufactory (no growth term), Austria's researched
    // `tw_growth_technologies` 5, and the tax-exempt flag set.
    let mut m = test_model_with(tax_growth_mapping().into_iter().chain([map_basic("tw_growth_technologies", "tw_growth_technologies")]));
    let mut rules = (*m.rules).clone();
    tax_normal_growth_bundles(&mut rules);
    m.rules = Arc::new(rules);
    with_technology(&mut m, &[("tw_growth_technologies", 5.0)]);
    {
        let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
        r.key = "eur_moravia".into();
        r.tax_exempt = true;
    }
    let fx = Effects::compute(&m);
    assert_eq!(economy::recompute_region_with(&m, Some(&fx), &m.world.regions[&RegionId(10)]).1, 5);
    // The same region taxed would lose the -0.55 x 5 modifier step and the -5 fixed term.
    m.world.regions.get_mut(&RegionId(10)).unwrap().tax_exempt = false;
    assert_eq!(economy::recompute_region_with(&m, Some(&fx), &m.world.regions[&RegionId(10)]).1, 5 - 2 - 5);
}

#[test]
fn tax_exempt_region_with_no_growth_term_stays_at_zero() {
    // eur_wallachia: a farm and a manufactory (gdp terms only), no growth term anywhere, and no tax
    // term because it is tax-exempt.
    let mut m = test_model_with(tax_growth_mapping());
    let mut rules = (*m.rules).clone();
    tax_normal_growth_bundles(&mut rules);
    m.rules = Arc::new(rules);
    {
        let r = m.world.regions.get_mut(&RegionId(11)).unwrap();
        r.key = "eur_wallachia".into();
        r.tax_exempt = true;
    }
    let fx = Effects::compute(&m);
    let (gdp, growth) = economy::recompute_region_with(&m, Some(&fx), &m.world.regions[&RegionId(11)]);
    assert_eq!((gdp, growth), (8000, 0));
    m.world.regions.get_mut(&RegionId(11)).unwrap().tax_exempt = false;
    assert_eq!(economy::recompute_region_with(&m, Some(&fx), &m.world.regions[&RegionId(11)]).1, -5);
}

#[test]
fn a_region_governed_by_another_faction_reads_that_factions_effects() {
    // eur_bavaria: owned by A, but B's governorship lists it, so the region reads B's government,
    // B's tax levels and B's faction-wide sum (B has no technology). Every building in its slots
    // counts even when another faction holds the slot.
    let mut m = test_model_with(tax_growth_mapping().into_iter().chain(["gdp_industry", "tw_growth_roads", "tw_growth_technologies", "gdp_mod_all"].map(|k| map_basic(k, k))));
    let mut rules = (*m.rules).clone();
    tax_normal_growth_bundles(&mut rules);
    rules.buildings.get_mut("test_building_level").unwrap().effects = vec![("gdp_industry".into(), 75.0), ("tw_growth_roads".into(), 4.0)];
    let rows: Vec<(String, f32)> = rules.buildings["test_building_level"].effects.clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, rows.iter().map(|(k, v)| (k.as_str(), *v)).collect()));
    m.rules = Arc::new(rules);
    with_technology(&mut m, &[("tw_growth_technologies", 5.0), ("gdp_mod_all", 3.0)]);
    {
        let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
        r.key = "eur_bavaria".into();
        r.base_gdp = 1200;
        r.slots[0].holder = Some(B);
    }
    // B's governorship lists the region, so B governs it (A does not).
    m.world
        .faction_details
        .entry(B)
        .or_default()
        .posts
        .push(super::details::GovernmentPost {
            id: 1,
            key: "governor_europe".into(),
            holder: None,
            governorship: Some(super::details::Governorship {
                taxes: super::details::GovernorshipTaxes {
                    lower: 2,
                    upper: 2,
                    lower_rate: 15,
                    upper_rate: 15,
                },
                theatre_id: 1,
                regions: vec![RegionId(10)],
                faction: B,
                flags: (false, false),
            }),
        });
    assert_eq!(m.world.governing_faction(RegionId(10)), Some(B));
    let fx = Effects::compute(&m);
    let (gdp, growth) = economy::recompute_region_with(&m, Some(&fx), &m.world.regions[&RegionId(10)]);
    // 1200 + 75 with no `gdp_mod_all` factor (Austria's 3 would give 77), and the roads' 4 taxed
    // with B's bundles: trunc(-0.55 x 4) = -2, 4 - 2 - 5 = -3.
    assert_eq!((gdp, growth), (1275, -3));
    // Without the governorship the region is Austria's again: the factor and the technology apply
    // (raw 9, trunc(-0.55 x 9) = -4, 9 - 4 - 5 = 0).
    m.world.faction_details.get_mut(&B).unwrap().posts.clear();
    let fx = Effects::compute(&m);
    let (gdp, growth) = economy::recompute_region_with(&m, Some(&fx), &m.world.regions[&RegionId(10)]);
    assert_eq!((gdp, growth), (1200 + 77, 0));
}

#[test]
fn command_validation_errors_leave_model_unchanged() {
    let mut m = test_model();
    let h = m.state_hash();
    let cases = [
        (
            CampaignCommand::SetTaxLevel { faction: FactionId(99), class: TaxClass::Lower, level: "tax_low".into() },
            CommandError::UnknownFaction(FactionId(99)),
        ),
        (
            CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Lower, level: "tax_silly".into() },
            CommandError::UnknownTaxLevel("tax_silly".into()),
        ),
        (
            CampaignCommand::Recruit { region: RegionId(99), unit_key: "test_unit".into(), target: None },
            CommandError::UnknownRegion(RegionId(99)),
        ),
        (CampaignCommand::Recruit { region: RegionId(10), unit_key: String::new(), target: None }, CommandError::EmptyUnitKey),
        (
            CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_nope".into(), target: None },
            CommandError::UnknownUnit("test_nope".into()),
        ),
        // Region 12 has no building, so nothing can be recruited there.
        (
            CampaignCommand::Recruit { region: RegionId(12), unit_key: "test_unit".into(), target: None },
            CommandError::UnitNotAvailable("test_unit".into()),
        ),
        (CampaignCommand::MoveForce { force: ForceId(99), to: pos(1, 1) }, CommandError::UnknownForce(ForceId(99))),
        (
            CampaignCommand::MoveForce { force: ForceId(1000), to: pos(40, 1) },
            // Straight line at road_level_0 cost 1.0: the only step costs 41 > 30.
            CommandError::NotEnoughMovementPoints { needed: 41, available: 30 },
        ),
        (CampaignCommand::DeclareWar { a: A, b: A }, CommandError::SameFaction(A)),
        (CampaignCommand::DeclareWar { a: A, b: FactionId(99) }, CommandError::UnknownFaction(FactionId(99))),
        (CampaignCommand::MakePeace { a: A, b: B }, CommandError::NotAtWar(A, B)),
        (CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }, CommandError::NotAtWar(A, B)),
        (CampaignCommand::MergeForces { force: ForceId(1000), into: ForceId(1001) }, CommandError::WrongFaction),
        (
            CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level".into() },
            CommandError::CannotBuild("test_building_level".into()),
        ),
        (
            CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level_2".into() },
            CommandError::CannotBuild("test_building_level_2".into()),
        ),
        (
            CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(7), level_key: "test_building_level".into() },
            CommandError::BadSlot(SlotRef::Slot(7)),
        ),
        (CampaignCommand::Autoresolve, CommandError::NoPendingBattle),
    ];
    for (cmd, err) in cases {
        assert_eq!(m.apply(cmd.clone()), Err(err), "{cmd:?}");
        assert_eq!(m.state_hash(), h, "a failed command must not change state: {cmd:?}");
    }

    // Not enough money: C has an empty treasury, so give it a region and try to recruit.
    m.world.regions.get_mut(&RegionId(10)).unwrap().owner = C;
    let h = m.state_hash();
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }),
        Err(CommandError::InsufficientFunds { needed: 400, available: 0 })
    );
    assert_eq!(m.state_hash(), h);

    // A force without a commander cannot move.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().commander = None;
    assert_eq!(
        m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(1, 0) }),
        Err(CommandError::NoCommander(ForceId(1000)))
    );
    assert_eq!(CommandError::NoCommander(ForceId(1000)).to_string(), "force 1000 has no commander");
}

#[test]
fn only_the_current_faction_may_act() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    assert_eq!(m.turn.current, Some(A));
    assert!(m.turn.in_turn);
    // B (before A in turn order) has played; it cannot act during A's turn.
    assert_eq!(
        m.apply(CampaignCommand::MoveForce { force: ForceId(1001), to: pos(1, 0) }),
        Err(CommandError::NotYourTurn(B))
    );
    assert!(m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(1, 0) }).is_ok());
}

#[test]
fn war_and_peace_are_symmetric() {
    let mut m = test_model();
    assert_eq!(m.world.stance(A, B), Stance::Neutral);
    let ev = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(ev, vec![CampaignEvent::StanceChanged { a: A, b: B, stance: Stance::War }]);
    assert_eq!(ev[0].script_name(), None);
    assert_eq!(m.world.stance(B, A), Stance::War);
    assert!(m.world.diplomacy_is_symmetric());
    assert_eq!(m.apply(CampaignCommand::DeclareWar { a: B, b: A }), Err(CommandError::AlreadyAtWar(B, A)));
    m.apply(CampaignCommand::MakePeace { a: B, b: A }).unwrap();
    assert_eq!(m.world.stance(A, B), Stance::Neutral);
    assert!(m.world.diplomacy_is_symmetric());
    assert_eq!(m.world.stance(A, C), Stance::Neutral);
    m.world.set_stance(A, C, Stance::Patron).unwrap();
    assert_eq!(m.world.stance(C, A), Stance::Protectorate);
    assert!(m.world.diplomacy_is_symmetric());
    m.world.factions.get_mut(&A).unwrap().diplomacy.insert(B, Stance::Allied);
    assert!(!m.world.diplomacy_is_symmetric());
}

#[test]
fn event_order_is_stable() {
    use CampaignEvent::*;
    let mut m = test_model();
    m.turn.humans = vec![A];
    let start = m.start_campaign();
    let r = RegionId;
    let ch = CharacterId;
    // Turn 1: round start (turn order B, A, C), B plays as AI, A starts and waits.
    // The faction turn start (0x008F2620, CONFIRMED order): characters, regions (slots, then RegionTurnStart),
    // then FactionTurnStart; the turn end (0x008BD0F0): characters, regions, units, FactionTurnEnd.
    let expected_start = vec![
        FactionRoundStart { faction: B },
        FactionRoundStart { faction: A },
        FactionRoundStart { faction: C },
        CharacterTurnStart { character: ch(101) },
        SlotTurnStart { region: r(11), slot: 0 },
        RegionTurnStart { region: r(11) },
        FactionTurnStart { faction: B },
        CharacterTurnEnd { character: ch(101) },
        RegionTurnEnd { region: r(11) },
        UnitTurnEnd { force: ForceId(1001), unit: UnitId(2) },
        UnitTurnEnd { force: ForceId(1001), unit: UnitId(3) },
        FactionTurnEnd { faction: B },
        CharacterTurnStart { character: ch(100) },
        CharacterTurnStart { character: ch(102) },
        SlotTurnStart { region: r(10), slot: 0 },
        SlotTurnStart { region: r(10), slot: 1 },
        RegionTurnStart { region: r(10) },
        RegionTurnStart { region: r(12) },
        FactionTurnStart { faction: A },
    ];
    assert_eq!(start, expected_start);
    assert_eq!(m.calendar.turns_elapsed, 0);
    // End turn: A ends, C plays, the round ends (calendar), round start, B plays, A starts.
    let end = m.end_turn();
    let names: Vec<_> = end.iter().filter_map(|e| e.script_name()).collect();
    assert_eq!(
        &names[..11],
        &[
            "CharacterTurnEnd",
            "CharacterTurnEnd",
            "RegionTurnEnd",
            "RegionTurnEnd",
            "UnitTurnEnd",
            "FactionTurnEnd",
            "FactionTurnStart",
            "FactionTurnEnd",
            "FactionRoundStart",
            "FactionRoundStart",
            "FactionRoundStart"
        ]
    );
    assert_eq!(end.last(), Some(&FactionTurnStart { faction: A }));
    assert_eq!(m.calendar.turns_elapsed, 1);
    assert_eq!(m.turn.current, Some(A));
    assert!(start.iter().all(|e| e.script_name().is_some()));
    let mut m2 = test_model();
    m2.turn.humans = vec![A];
    assert_eq!(m2.start_campaign(), expected_start);
}

#[test]
fn stepping_gives_the_same_events_as_running() {
    let mut a = test_model();
    let mut b = test_model();
    let all = a.end_turn();
    let mut stepped = Vec::new();
    b.begin_end_turn(); // not started yet: this queues the start
    while let Some(mut ev) = b.step() {
        stepped.append(&mut ev);
    }
    b.begin_end_turn();
    while let Some(mut ev) = b.step() {
        stepped.append(&mut ev);
    }
    assert_eq!(all, stepped);
    assert_eq!(a, b);
}

/// A raised unit's full size (`world::recruited_unit_size`, CONFIRMED from the vanilla saves in
/// `ntw_campaign`'s `economy_fidelity`): `num_men` for land, the crew triple's sum for a ship, even
/// when no unit of that ship type exists yet.
#[test]
fn recruited_units_are_sized_by_num_men_or_the_crew_sum() {
    use super::naval::ShipRules;
    let mut rules = CampaignRules::test_rules();
    rules.units.insert("test_ship".into(), UnitRules { is_naval: true, men: 0, ..rules.units["test_unit"].clone() });
    rules.ships.insert("test_ship".into(), ShipRules { crews: [20, 20, 100], ..Default::default() });
    assert_eq!(world::recruited_unit_size(&rules, "test_unit"), 100);
    assert_eq!(world::recruited_unit_size(&rules, "test_ship"), 140);
    assert_eq!(world::recruited_unit_size(&rules, "no_such_unit"), 1);
    // Through the spawn: a ship type no force holds yet still gets its crew sum.
    let mut m = test_model();
    m.rules = Arc::new(rules);
    let CampaignEvent::UnitTrained { force, unit } = m.spawn_recruited_unit(RegionId(10), "test_ship".into()) else {
        panic!("a unit is trained")
    };
    let u = m.world.forces[&force].units.iter().find(|u| u.id == unit).expect("the new unit");
    assert_eq!((u.men, u.max_men), (140, 140));
}

#[test]
fn recruitment_completes_after_n_turns() {
    let mut m = test_model();
    // MADE-UP rules: campaign cost (#7) 400, 2 turns, 100 men; region 10 has 2 recruitment points.
    let ev = m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    assert_eq!(ev, vec![CampaignEvent::RecruitmentItemIssuedByPlayer { region: RegionId(10) }]);
    assert_eq!(m.world.factions[&A].treasury, 600);
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 200);
    m.world.factions.get_mut(&A).unwrap().treasury = 10_000;
    // A third item queues too (a queue holds 10, 0x00B62040); it would wait while both points train.
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 9_600);
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 2) }).unwrap();
    // Cancelling refunds.
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 1) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 10_400);
    m.turn.humans = vec![A];
    let ev1 = m.start_campaign();
    assert!(!ev1.iter().any(|e| matches!(e, CampaignEvent::UnitTrained { .. })));
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue[0].turns_remaining, 1);
    let ids_before: Vec<u32> = m.world.characters.keys().map(|c| c.0 as u32).chain(m.world.forces.keys().map(|f| f.0)).collect();
    let ev2 = m.end_turn();
    let i_trained = ev2.iter().position(|e| matches!(e, CampaignEvent::UnitTrained { .. })).unwrap();
    let CampaignEvent::UnitTrained { force, unit } = ev2[i_trained] else { unreachable!() };
    let i_end = ev2.iter().position(|e| *e == CampaignEvent::RegionTurnStart { region: RegionId(10) }).unwrap();
    assert!(i_trained < i_end);
    assert!(m.world.regions[&RegionId(10)].recruitment_queue.is_empty());
    // The settlement had no garrison: a new army led by a new colonel, garrisoned there, with the
    // colonel attached to the unit (SAVE_COMPAT.md §4).
    assert_eq!(m.world.regions[&RegionId(10)].garrison, Some(force));
    let garrison = m.world.forces[&force].clone();
    let colonel = garrison.commander.expect("every force has a commander");
    assert_eq!(garrison.faction, A);
    assert_eq!(garrison.units[0].id, unit);
    assert_eq!(garrison.units[0].unit_key, "test_recruit");
    assert_eq!(garrison.units[0].men, 100);
    assert_eq!(garrison.units[0].character, Some(colonel));
    let ch = &m.world.characters[&colonel];
    assert_eq!((ch.kind, ch.faction, ch.garrisoned_in), (CharacterKind::Colonel, A, Some(RegionId(10))));
    assert_eq!(ch.position, m.world.regions[&RegionId(10)].settlement.position);
    // New ids: unique, above every id loaded, steps of 8.
    for id in [unit.0 as u32, force.0, colonel.0 as u32] {
        assert!(ids_before.iter().all(|&b| b < id), "{id}");
        assert_eq!(id % 8, 0);
    }
    // The next unit raised there joins the garrison, with no colonel of its own.
    let n = m.world.forces.len();
    m.spawn_recruited_unit(RegionId(10), "test_unit".into());
    assert_eq!(m.world.forces.len(), n);
    assert_eq!(m.world.forces[&force].units.len(), 2);
    assert_eq!(m.world.forces[&force].units[1].character, None);
}

/// `0x00B0D220`, the recruitable entry's cost (CONFIRMED): `units` #7 (not the #4 cost) scaled by
/// `max(−100, mod) + 100` percent in f32 and rounded by FISTP (half to even), `mod` being the integer
/// `recruitment_mod_cost_land_all` (`_naval_all` for a ship) plus the unit category's and class's
/// `cost_mod`, and the campaign's unit-key suffix cost mod (spa_napoleon's guerrilla / auxiliary).
#[test]
fn the_recruitment_cost_is_units_7_scaled_by_the_cost_effects() {
    use super::effects::EffectSet;
    use super::features::UnitKeyEffects;
    let mut rules = CampaignRules::test_rules();
    let land = UnitRules { cost: 710, campaign_cost: 580, category: "infantry".into(), unit_class: "infantry_line".into(), ..Default::default() };
    let ship = UnitRules { is_naval: true, category: "naval_frigate".into(), unit_class: "frigate".into(), ..land.clone() };
    let mut set = EffectSet::default();
    // No effects: the #7 cost exactly.
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &land), 580);
    // −8 (a real vanilla figure: Sweden's line infantry was queued at 534, its hussars at 432).
    set.add(EffectKey::basic("recruitment_mod_cost_land_all"), -8.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &land), 534);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &UnitRules { campaign_cost: 470, ..land.clone() }), 432);
    // A ship reads the naval key only.
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &ship), 580);
    set.add(EffectKey::basic("recruitment_mod_cost_naval_all"), -1.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &UnitRules { campaign_cost: 500, ..ship.clone() }), 495);
    // The category's and the class's cost_mod add in.
    set.add(EffectKey { kind: BonusKind::UnitCategory, bonus: "cost_mod".into(), qualifier: "infantry".into() }, -2.0);
    set.add(EffectKey { kind: BonusKind::UnitClass, bonus: "cost_mod".into(), qualifier: "infantry_line".into() }, -10.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &land), 464); // 580 × 0.80
    // Each effect is read as an integer, half to even: −0.5 reads 0, −1.5 reads −2.
    let mut half = EffectSet::default();
    half.add(EffectKey::basic("recruitment_mod_cost_land_all"), -0.5);
    assert_eq!(economy::recruitment_cost_in(&rules, &half, "u", &land), 580);
    half.add(EffectKey { kind: BonusKind::UnitCategory, bonus: "cost_mod".into(), qualifier: "infantry".into() }, -1.5);
    assert_eq!(economy::recruitment_cost_in(&rules, &half, "u", &land), 568); // 580 × 0.98 = 568.4
    // The product is rounded half to even: 25 × 0.5 = 12.5 → 12, 35 × 0.5 = 17.5 → 18.
    let mut fifty = EffectSet::default();
    fifty.add(EffectKey::basic("recruitment_mod_cost_land_all"), -50.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &fifty, "u", &UnitRules { campaign_cost: 25, ..land.clone() }), 12);
    assert_eq!(economy::recruitment_cost_in(&rules, &fifty, "u", &UnitRules { campaign_cost: 35, ..land.clone() }), 18);
    // The sum is clamped at −100: never a negative price.
    fifty.add(EffectKey::basic("recruitment_mod_cost_land_all"), -500.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &fifty, "u", &land), 0);
    // The guerrilla / auxiliary mods count only with the campaign's suffix table (spa_napoleon's), by the end of
    // the unit key, the first match only.
    let mut spa = EffectSet::default();
    spa.add(EffectKey::basic("guerrilla_cost_mod"), -20.0);
    spa.add(EffectKey::basic("auxiliary_cost_mod"), 10.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_Guerrilla", &land), 580);
    rules.features.unit_key_effects = vec![UnitKeyEffects::new("_Guerrilla", "guerrilla_cost_mod", "guerrilla_upkeep_mod"), UnitKeyEffects::new("_Auxiliary", "auxiliary_cost_mod", "auxiliary_upkeep_mod")];
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_Guerrilla", &land), 464);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_British_Auxiliary", &land), 638);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Guerrilla_Line", &land), 580);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_guerrilla", &land), 580);
    // Another campaign's own suffix (a Shogun-style roster): any key, any effect.
    rules.features.unit_key_effects = vec![UnitKeyEffects::new("_Ashigaru", "auxiliary_cost_mod", "auxiliary_upkeep_mod")];
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Yari_Ashigaru", &land), 638);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_Guerrilla", &land), 580);
}

/// A unit's upkeep adds the campaign's unit-key upkeep effect (land `0x008F9B5F`, naval `0x008F9D4F`; spa: `_Guerrilla` →
/// `guerrilla_upkeep_mod`, else `_Auxiliary` → `auxiliary_upkeep_mod`; review: missing). A campaign
/// without the feature is unchanged.
#[test]
fn upkeep_adds_the_unit_key_effect() {
    use super::effects::{EffectKey, EffectSet, Effects};
    use super::features::{CampaignFeatures, UnitKeyEffects};
    let mut set = EffectSet::default();
    set.add(EffectKey::basic("guerrilla_upkeep_mod"), -50.0);
    set.add(EffectKey::basic("auxiliary_upkeep_mod"), 20.0);
    let mut fx = Effects::default();
    fx.faction.insert(A, set);
    let land = UnitRules { upkeep: 100, ..Default::default() };
    let none = CampaignFeatures::default();
    assert_eq!(economy::unit_upkeep(&fx, A, &none, "Inf_Spanish_Guerrilla", &land), 100);
    let spa = CampaignFeatures {
        unit_key_effects: vec![UnitKeyEffects::new("_Guerrilla", "guerrilla_cost_mod", "guerrilla_upkeep_mod"), UnitKeyEffects::new("_Auxiliary", "auxiliary_cost_mod", "auxiliary_upkeep_mod")],
        ..Default::default()
    };
    assert_eq!(economy::unit_upkeep(&fx, A, &spa, "Inf_Spanish_Guerrilla", &land), 50);
    assert_eq!(economy::unit_upkeep(&fx, A, &spa, "Inf_British_Auxiliary", &land), 120);
    assert_eq!(economy::unit_upkeep(&fx, A, &spa, "Inf_Line", &land), 100);
    let ship = UnitRules { upkeep: 100, is_naval: true, ..Default::default() };
    // Ships too (`0x008F9D00`, review round 2), and a ship's negative mod costs 0 where a land unit's goes negative.
    assert_eq!(economy::unit_upkeep(&fx, A, &spa, "Ship_Guerrilla", &ship), 50);
    let mut cheap = EffectSet::default();
    cheap.add(EffectKey::basic("upkeep_cost_mod_naval_all"), -150.0);
    cheap.add(EffectKey::basic("upkeep_cost_mod_land_all"), -150.0);
    fx.faction.insert(A, cheap);
    assert_eq!(economy::unit_upkeep(&fx, A, &spa, "Ship", &ship), 0);
    assert_eq!(economy::unit_upkeep(&fx, A, &spa, "Inf_Line", &land), -50);
}

/// The recruit command charges the region's entry cost (#7 with the region's effects, here a faction
/// `recruitment_mod_cost_land_all` of −8 from its saved bonuses), the item keeps it, and a cancel
/// credits exactly that back (`0x00B5C060`: item +0x20 as income category 3).
#[test]
fn recruiting_charges_the_entry_cost_and_cancelling_refunds_it() {
    let mut m = test_model();
    let unit = m.rules.units["test_recruit"].clone();
    assert_ne!(unit.cost, unit.campaign_cost, "the fixture tells the two costs apart");
    m.world.faction_details.entry(A).or_default().bonus_base = vec![SavedBonus { kind: 1, bonus: 30, value: -8.0, qualifier: String::new() }];
    let before = m.world.factions[&A].treasury;
    let cost = (unit.campaign_cost as f32 * 0.92).round_ties_even() as i32;
    assert_eq!(economy::recruitment_cost(&m, &m.world.regions[&RegionId(10)], "test_recruit", &unit), cost);
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, before - cost);
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue[0].cost, cost);
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, before);
}

/// The entry is "too dear" only when its cost is above the treasury compared unsigned (`0x00B69BA0`,
/// flag 2), and the command has no other money test: a faction in debt may recruit, going deeper into
/// debt; one with a positive treasury below the cost may not.
#[test]
fn a_faction_in_debt_may_recruit() {
    let mut m = test_model();
    let cost = m.rules.units["test_recruit"].campaign_cost;
    m.world.factions.get_mut(&A).unwrap().treasury = cost - 1;
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }),
        Err(CommandError::InsufficientFunds { needed: cost, available: cost - 1 })
    );
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, -5 - cost);
}

/// The recruit command (`0x00936B90` → `0x00B58DD0`) does not look at the recruitment points: a region
/// with none still queues, and its items then wait (`0x00B71FB0` counts down only the first
/// `recruitment_points` items); only a full queue (10) refuses.
#[test]
fn a_region_without_recruitment_points_still_queues() {
    let mut m = test_model();
    // The region's building gives 2 points; a faction effect of −2 takes them away.
    m.world.faction_details.entry(A).or_default().bonus_base = vec![SavedBonus { kind: 1, bonus: 32, value: -2.0, qualifier: String::new() }];
    assert_eq!(m.recruitment_points(RegionId(10), false), 0);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    let turns = m.world.regions[&RegionId(10)].recruitment_queue[0].turns_remaining;
    m.turn.humans = vec![A];
    m.start_campaign();
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue[0].turns_remaining, turns);
    while m.world.regions[&RegionId(10)].recruitment_queue.len() < super::commands::MAX_QUEUE as usize {
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    }
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }),
        Err(CommandError::NoRecruitmentCapacity)
    );
}

/// `units` #15 caps what a faction may hold and have queued of one unit type (`0x008F68B0`: the faction's
/// live units of the type plus its queued items of it, against the cap; flag 0x40 of `0x00B69BA0`). Other
/// factions' units and queues do not count, a cancel or a lost unit frees a place, and 0 means no cap.
#[test]
fn the_unit_cap_counts_the_factions_units_and_queued_items() {
    use super::commands::{ENTRY_QUEUE_FULL, ENTRY_TOO_DEAR, ENTRY_UNIT_CAP};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_recruit").unwrap().unit_cap = 3;
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    let recruit = |k: &str| CampaignUnit { unit_key: k.into(), ..unit(50) };
    // A holds one; B holds two and has one queued, which are B's own.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.push(recruit("test_recruit"));
    m.world.forces.get_mut(&ForceId(1001)).unwrap().units.extend([recruit("test_recruit"), recruit("test_recruit")]);
    m.world.regions.get_mut(&RegionId(11)).unwrap().recruitment_queue.push(RecruitmentItem {
        id: RecruitmentItemId(9000),
        unit_key: "test_recruit".into(),
        turns_remaining: 2,
        cost: 400,
        target: None,
    });
    let order = || CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None };
    // 1 held + 0 and then 1 queued are below 3; at 1 + 2 the entry is flagged.
    m.apply(order()).unwrap();
    m.apply(order()).unwrap();
    assert_eq!(m.apply(order()), Err(CommandError::UnitCapReached("test_recruit".into())));
    let unit = m.rules.units["test_recruit"].clone();
    let flags = |m: &CampaignModel| m.recruitable_entry_flags(&m.world.regions[&RegionId(10)], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags(&m), ENTRY_UNIT_CAP);
    // A cancel frees a place.
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 1) }).unwrap();
    assert_eq!(flags(&m), 0);
    m.apply(order()).unwrap();
    // So does a unit the faction no longer holds.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.retain(|u| u.unit_key != "test_recruit");
    assert_eq!(flags(&m), 0);
    m.apply(order()).unwrap();
    assert_eq!(flags(&m), ENTRY_UNIT_CAP);
    // The flags add up: also too dear and, once the queue holds 10, full.
    m.world.factions.get_mut(&A).unwrap().treasury = 399;
    assert_eq!(flags(&m), ENTRY_UNIT_CAP | ENTRY_TOO_DEAR);
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_recruit").unwrap().unit_cap = 0;
    m.rules = Arc::new(rules);
    let unit = m.rules.units["test_recruit"].clone();
    let flags = |m: &CampaignModel| m.recruitable_entry_flags(&m.world.regions[&RegionId(10)], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags(&m), ENTRY_TOO_DEAR, "a cap of 0 is no cap");
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    while m.world.regions[&RegionId(10)].recruitment_queue.len() < super::commands::MAX_QUEUE as usize {
        m.apply(order()).unwrap();
    }
    assert_eq!(flags(&m), ENTRY_QUEUE_FULL);
}

/// The recruitable population (`0x00A89550` / `0x00AAF190` / `0x00A61AA0`): with the shipped variables (both
/// 0) a recruit needs and takes nothing; with a mod's `recruitment_population_cost` (rounded half to even, as
/// `0x008B25F0` reads it) and `minimum_population_after_recruitment`, the queue command takes the cost from
/// the region's population while that leaves the minimum, flags the entry (4) and refuses below it without
/// charging anything, and every untrained exit (cancel, the turn start's removal, a capture) gives the cost
/// back per item.
#[test]
fn recruiting_takes_population_and_cancelling_gives_it_back() {
    use super::commands::ENTRY_NO_POPULATION;
    let mut m = test_model();
    let r10 = RegionId(10);
    let order = || CampaignCommand::Recruit { region: r10, unit_key: "test_recruit".into(), target: None };
    let pop = |m: &CampaignModel| m.world.regions[&r10].population;
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m.world.regions.get_mut(&r10).unwrap().population = 5;
    m.apply(order()).unwrap();
    assert_eq!(pop(&m), 5, "the shipped variables are 0");

    let mut rules = (*m.rules).clone();
    rules.variables.insert("recruitment_population_cost".into(), 300.5);
    rules.variables.insert("minimum_population_after_recruitment".into(), 1000.0);
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&r10).unwrap().population = 1600;
    m.apply(order()).unwrap();
    assert_eq!(pop(&m), 1300, "300.5 reads as 300");
    m.apply(order()).unwrap();
    assert_eq!(pop(&m), 1000, "exactly the minimum is left");
    let unit = m.rules.units["test_recruit"].clone();
    let flags = |m: &CampaignModel| m.recruitable_entry_flags(&m.world.regions[&r10], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags(&m), ENTRY_NO_POPULATION);
    let treasury = m.world.factions[&A].treasury;
    assert_eq!(m.apply(order()), Err(CommandError::NotEnoughPopulation));
    assert_eq!((pop(&m), m.world.factions[&A].treasury, m.world.regions[&r10].recruitment_queue.len()), (1000, treasury, 3));

    // A cancel gives the cost back.
    m.apply(CampaignCommand::CancelRecruitment { region: r10, item: queued(&m, r10, 2) }).unwrap();
    assert_eq!(pop(&m), 1300);
    assert_eq!(flags(&m), 0);

    // So does the turn start's removal of an item the region can no longer recruit, per item.
    for id in [9001, 9002] {
        let item = RecruitmentItem { id: RecruitmentItemId(id), unit_key: "gone_unit".into(), turns_remaining: 5, cost: 0, target: None };
        m.world.regions.get_mut(&r10).unwrap().recruitment_queue.push(item);
    }
    assert!(!m.recruitable_units(r10).iter().any(|e| e.unit_key == "gone_unit"));
    m.turn.humans = vec![A];
    m.start_campaign();
    assert_eq!((pop(&m), m.world.regions[&r10].recruitment_queue.len()), (1900, 2));

    // And a capture, for every item it clears.
    let mut events = Vec::new();
    m.occupy(r10, B, None, &mut events);
    assert_eq!((pop(&m), m.world.regions[&r10].recruitment_queue.len()), (2500, 0));
}

/// Region 10 (owner A) with two land items (costs 300, 500), two ship items (70, 110) and a construction
/// (600), each cost as stored when it was paid.
fn model_with_queued_items() -> CampaignModel {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.units.insert("test_ship".into(), UnitRules { is_naval: true, ..rules.units["test_unit"].clone() });
    m.rules = Arc::new(rules);
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    for (id, unit_key, cost) in [(9001, "test_unit", 300), (9002, "test_ship", 70), (9003, "test_unit", 500), (9004, "test_ship", 110)] {
        r.recruitment_queue.push(RecruitmentItem { id: RecruitmentItemId(id), unit_key: unit_key.into(), turns_remaining: 2, cost, target: None });
    }
    r.construction.push(ConstructionItem { slot: SlotRef::Slot(0), level_key: "test_building_level".into(), turns_remaining: 2, cost: 600 });
    m
}

/// A capture (every capture variant, e.g. `0x00B58A30` / `0x00B58560`) empties the land queue with no refund
/// (`0x00B1A760(0)` on region +0x124), each port's naval queue with the refund (`0x00B1A760(1)` on slot
/// +0x1E8: `0x00B5C0A0` credits item +0x20 to the region's owner, still the old one) and every construction
/// with none (`0x00A6CBE0(0)`), whoever the old owner is.
#[test]
fn a_capture_refunds_the_old_owners_queued_ships_only() {
    for human in [false, true] {
        let mut m = model_with_queued_items();
        if human {
            m.turn.humans = vec![A];
        }
        let (a, b) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
        let mut events = Vec::new();
        m.occupy(RegionId(10), B, None, &mut events);
        let r = &m.world.regions[&RegionId(10)];
        assert!(r.recruitment_queue.is_empty() && r.construction.is_empty());
        assert_eq!(m.world.factions[&A].treasury, a + 70 + 110, "human old owner: {human}");
        assert_eq!(m.world.factions[&B].treasury, b);
    }
}

/// A region handed over without a capture (a deal's region item, a liberation, `grant_faction_handover`:
/// `0x00B58A10` → `0x00B449F0` with the report flag clear) runs only `0x00A64AC0`'s drain, whose flag is set
/// when the old owner is not human (faction +0x6E0): an AI old owner gets every queued unit's cost back, land
/// and naval; a human one gets nothing. Constructions are never refunded.
#[test]
fn a_transfer_refunds_an_ai_old_owners_queued_units_and_a_humans_none() {
    for (human, refund) in [(false, 300 + 70 + 500 + 110), (true, 0)] {
        let mut m = model_with_queued_items();
        if human {
            m.turn.humans = vec![A];
        }
        let (a, b) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
        assert!(m.transfer_region(RegionId(10), B));
        let r = &m.world.regions[&RegionId(10)];
        assert!(r.recruitment_queue.is_empty() && r.construction.is_empty());
        assert_eq!(m.world.factions[&A].treasury, a + refund, "human old owner: {human}");
        assert_eq!(m.world.factions[&B].treasury, b);
    }
}

/// The population rules' 32-bit arithmetic (`0x00AAF190`): a charge that would leave less than the minimum
/// sets the population to the minimum (unreachable through the queue command, which refuses that entry
/// first); the gate's sum and compare are signed, so a mod's negative cost passes any population.
#[test]
fn the_recruitment_population_rules_are_signed_32_bit() {
    use super::population::RecruitmentPopulation;
    let p = RecruitmentPopulation { cost: 300, minimum: 1000 };
    assert_eq!((p.available(1299), p.charged(1299), p.charged(1300)), (false, 1000, 1000));
    assert_eq!(p.credited(1000, 3), 1900);
    let negative = RecruitmentPopulation { cost: -50, minimum: 0 };
    assert!(negative.available(0));
    assert_eq!((negative.charged(0), negative.credited(100, 2)), (50, 0));
    let mut rules = super::rules::CampaignRules::default();
    rules.variables.insert("recruitment_population_cost".into(), 2.5);
    rules.variables.insert("minimum_population_after_recruitment".into(), 3.5);
    assert_eq!(RecruitmentPopulation::of(&rules), RecruitmentPopulation { cost: 2, minimum: 4 });
    assert_eq!(RecruitmentPopulation::of(&super::rules::CampaignRules::default()), RecruitmentPopulation { cost: 0, minimum: 0 });
}

/// A mod's negative `minimum_population_after_recruitment` lets recruiting take the population below 0. The exe
/// keeps the field's 32 bits: the gate keeps reading them signed (so −200 still passes a need of −700 and −800
/// fails it), while the growth reads them unsigned (`0x00AB4227`), as the `u32` holding the same bits does here.
#[test]
fn a_population_charged_below_zero_keeps_the_exes_bits() {
    use super::commands::ENTRY_NO_POPULATION;
    let mut m = test_model();
    let r10 = RegionId(10);
    let order = || CampaignCommand::Recruit { region: r10, unit_key: "test_recruit".into(), target: None };
    let mut rules = (*m.rules).clone();
    rules.variables.insert("recruitment_population_cost".into(), 300.0);
    rules.variables.insert("minimum_population_after_recruitment".into(), -1000.0);
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m.world.regions.get_mut(&r10).unwrap().population = 100;
    m.apply(order()).unwrap();
    assert_eq!(m.world.regions[&r10].population, (-200i32) as u32);
    m.apply(order()).unwrap();
    m.apply(order()).unwrap();
    assert_eq!(m.world.regions[&r10].population, (-800i32) as u32);
    let unit = m.rules.units["test_recruit"].clone();
    assert_eq!(m.recruitable_entry_flags(&m.world.regions[&r10], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A)), ENTRY_NO_POPULATION);
    assert_eq!(m.apply(order()), Err(CommandError::NotEnoughPopulation));
    // Growth of 1% on the bits of −200, read unsigned: f32(4294967096) = 4294967040, × 0.01 → 42949668 (FISTP),
    // plus −200 (a signed read would give −202).
    let r = &m.world.regions[&r10];
    let state = super::population::PopulationState { factors: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
    assert_eq!(super::population::grow(&m, r, (-200i32) as u32, &state).0, 42_949_468);
}

/// The ranking's power (`0x008B2150` with flag 0) sums each unit's raw upkeep: no effects, whatever
/// the unit's strength (A: one test_unit, B: two; 80 of 100 men each). A unit with no record counts
/// 0 and is reported. A navy's units go to the naval total, an army's to the land total.
#[test]
fn faction_power_is_the_raw_upkeep_of_the_forces() {
    use super::negotiation::FactionPower;
    let mut m = test_model();
    let mut unknown = std::collections::BTreeSet::new();
    let p = m.faction_powers(&mut unknown);
    let land = |land| Some(FactionPower { land, naval: 0 });
    assert_eq!((p.get(&A).copied(), p.get(&B).copied(), p.get(&C).copied()), (land(10), land(20), None));
    assert_eq!(p[&B].total(), 20);
    assert!(unknown.is_empty());
    m.world.forces.get_mut(&ForceId(1000)).unwrap().is_navy = true;
    assert_eq!(m.faction_powers(&mut unknown)[&A], FactionPower { land: 0, naval: 10 });
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].unit_key = "no_such_unit".into();
    let r = m.faction_rankings();
    assert_eq!(r.unknown_units.iter().map(String::as_str).collect::<Vec<_>>(), ["no_such_unit"]);
    assert!(r.ranks.contains_key(&A));
}

/// The negotiation's constructor (`0x00BF5A60`, run by the `BeginNegotiation` command) picks the
/// human proposer's greeting (the recipient's `receive` line) and the human recipient's `approach`
/// line; an override row costs one RNG draw, no override row none, and reading the negotiation
/// draws nothing. `EndNegotiation` ends it once.
#[test]
fn a_negotiation_picks_its_greetings_once() {
    use super::negotiation::Greeting;
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.factions.get_mut(&B).unwrap().government_key = "gov_republic".into();
    let mut rules = (*m.rules).clone();
    rules.faction_cultures.insert("test_faction_b".into(), "european".into());
    for attitude in ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"] {
        let key = (format!("receive_{attitude}"), "european".to_owned(), "gov_republic".to_owned());
        rules.negotiation_strings.insert(key.clone(), "generic".into());
        rules.negotiation_overrides.insert((key.0, key.1, key.2, "test_faction_b".into()), "own".into());
    }
    m.rules = Arc::new(rules);
    // A (human) proposes to B: B's receive line, one draw for its override row.
    let before = m.rng;
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let mut once = before;
    let pick = if once.unit_float() < 0.5 { "own" } else { "generic" };
    assert_eq!(m.rng, once);
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!((n.serial, n.proposer, n.recipient), (1, A, B));
    assert_eq!(n.greeting, Some(Greeting::Line(pick.into())));
    assert_eq!(n.approach, None, "B is not human");
    let _ = m.faction_rankings();
    assert_eq!(m.rng, once, "reading draws nothing");
    // B proposes to A (human), over the open one: no greeting; B's approach line has no row:
    // Missing, no draw. Building over it ends nothing.
    m.apply(CampaignCommand::BeginNegotiation { proposer: B, recipient: A }).unwrap();
    assert_eq!(m.rng, once);
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!((n.serial, n.greeting), (2, None));
    let key = ("approach_neutral".to_owned(), "european".to_owned(), "gov_republic".to_owned());
    assert_eq!(n.approach, Some(Greeting::Missing { faction: "test_faction_b".into(), key }), "the log names the faction and the row");
    assert_eq!(m.negotiations.ended, 0);
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    assert_eq!((m.negotiations.current.is_none(), m.negotiations.begun, m.negotiations.ended), (true, 2, 1), "ended once");
    assert_eq!(
        m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: FactionId(999) }),
        Err(CommandError::UnknownFaction(FactionId(999)))
    );
    assert_eq!(m.negotiations.begun, 2, "a rejected begin leaves no negotiation");
}

#[test]
fn merging_takes_the_commander_along_and_destroying_kills_attached_characters() {
    let mut m = test_model();
    // Force 1000 (commander 100) has unit 1; give the commander his bodyguard link.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].character = Some(CharacterId(100));
    let mut events = Vec::new();
    // Merge force 1000 into a second force of faction A.
    m.world.characters.insert(CharacterId(105), character(105, A, CharacterKind::General, 26));
    m.world.forces.insert(
        ForceId(1010),
        MilitaryForce { id: ForceId(1010), faction: A, commander: Some(CharacterId(105)), units: vec![unit(20)], is_navy: false },
    );
    m.merge_units(ForceId(1000), ForceId(1010), &mut events);
    assert!(!m.world.forces.contains_key(&ForceId(1000)));
    let into = &m.world.forces[&ForceId(1010)];
    assert_eq!(into.units.len(), 2);
    assert_eq!(into.units[1].character, Some(CharacterId(100)));
    // The former commander lives on, attached to his unit, at the receiving army's spot.
    assert_eq!(m.world.characters[&CharacterId(100)].position, m.world.characters[&CharacterId(105)].position);
    assert!(m.force_of(CharacterId(100)).is_none());
    // Destroying the receiving force kills its commander and the attached general.
    m.destroy_force(ForceId(1010), &mut events);
    assert!(!m.world.characters.contains_key(&CharacterId(105)));
    assert!(!m.world.characters.contains_key(&CharacterId(100)));
}

#[test]
fn construction_completes_and_unlocks_upgrades() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    let t0 = m.world.factions[&A].treasury;
    // Slot 1 is empty: only level 0 of a chain fits.
    let ev = m
        .apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level".into() })
        .unwrap();
    assert_eq!(ev, vec![CampaignEvent::BuildingConstructionIssuedByPlayer { region: RegionId(10) }]);
    assert_eq!(m.world.factions[&A].treasury, t0 - 300);
    assert_eq!(
        m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level".into() }),
        Err(CommandError::SlotBusy)
    );
    // Slot 0 holds level 0: its upgrade is allowed (3 turns).
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level_2".into() })
        .unwrap();
    m.end_turn();
    assert!(m.world.regions[&RegionId(10)].slots[1].building.is_none());
    let ev = m.end_turn();
    assert!(ev.contains(&CampaignEvent::BuildingCompleted { region: RegionId(10), slot: SlotRef::Slot(1) }));
    assert_eq!(m.world.regions[&RegionId(10)].slots[1].building.as_ref().unwrap().level_key, "test_building_level");
    m.end_turn();
    assert_eq!(m.world.regions[&RegionId(10)].slots[0].building.as_ref().unwrap().level_key, "test_building_level_2");
    assert!(m.world.regions[&RegionId(10)].construction.is_empty());
}

#[test]
fn move_force_spends_and_end_turn_refills_movement_points() {
    let mut m = test_model();
    let ev = m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(18, 24) }).unwrap();
    assert_eq!(ev.len(), 2);
    assert!(matches!(&ev[0], CampaignEvent::CharacterMoved { character: CharacterId(100), path } if path.len() == 2));
    assert_eq!(ev[1], CampaignEvent::MovementPointsExhausted { character: CharacterId(100) });
    let general = &m.world.characters[&CharacterId(100)];
    assert_eq!(general.movement_points, 0);
    assert_eq!(general.position, pos(18, 24));
    assert!(m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(19, 24) }).is_err());
    m.end_turn();
    assert_eq!(m.world.characters[&CharacterId(100)].movement_points, 30);
}

#[test]
fn attack_and_autoresolve() {
    fn fight() -> (CampaignModel, Vec<CampaignEvent>) {
        let mut m = test_model();
        m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(5, 0);
        m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
        let ev = m.apply(CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }).unwrap();
        assert!(ev.contains(&CampaignEvent::PreBattle { attacker: ForceId(1000) }));
        assert!(m.pending_battle.is_some());
        // Nothing else may happen until the battle is fought.
        assert_eq!(m.apply(CampaignCommand::EndTurn), Err(CommandError::BattlePending));
        let ev = m.apply(CampaignCommand::Autoresolve).unwrap();
        (m, ev)
    }
    let (m, ev) = fight();
    // 80 men against 160 of the same quality: B wins.
    assert!(matches!(ev[0], CampaignEvent::BattleCompleted { attacker: ForceId(1000), attacker_won: false, .. }));
    assert!(m.pending_battle.is_none());
    let men_b: u32 = m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).sum();
    assert!(men_b < 160);
    // Deterministic.
    assert_eq!(m.state_hash(), fight().0.state_hash());
}

#[test]
fn enter_and_occupy_settlements_and_merge() {
    let mut m = test_model();
    // Own settlement (region 12 at x = 12): enter it.
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(10, 0);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(12) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEntersGarrison { character: CharacterId(100), region: RegionId(12) }));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, Some(RegionId(12)));
    // An undefended enemy settlement (region 11 at x = 11) is occupied at once.
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(-20, 0);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::SettlementOccupied { region: RegionId(11), faction: A }));
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    // Merge: a second army of A joins the first.
    let mut g = character(103, A, CharacterKind::General, 30);
    g.position = pos(11, 1);
    m.world.characters.insert(CharacterId(103), g);
    m.world.forces.insert(
        ForceId(1005),
        MilitaryForce { id: ForceId(1005), faction: A, commander: Some(CharacterId(103)), units: vec![unit(9)], is_navy: false },
    );
    let ev = m.apply(CampaignCommand::MergeForces { force: ForceId(1005), into: ForceId(1000) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CampaignArmiesMerge { force: ForceId(1005), into: ForceId(1000) }));
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 2);
    assert!(!m.world.forces.contains_key(&ForceId(1005)));
}

#[test]
fn units_per_army_come_from_the_campaign_and_a_mod() {
    // A full 20-unit army (the original's cap) takes no more; the campaign's own value and a new
    // campaign under a mod's `max_land_units` row both lift it.
    let setup = |extra: usize| {
        let mut m = test_model();
        m.world.forces.get_mut(&ForceId(1000)).unwrap().units = (0..20).map(|i| unit(500 + i)).collect();
        let mut g = character(103, A, CharacterKind::General, 30);
        g.position = m.world.characters[&CharacterId(100)].position;
        m.world.characters.insert(CharacterId(103), g);
        let units = (0..extra as i32).map(|i| unit(600 + i)).collect();
        m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: A, commander: Some(CharacterId(103)), units, is_navy: false });
        m
    };
    let merge = CampaignCommand::MergeForces { force: ForceId(1005), into: ForceId(1000) };
    let mut m = setup(1);
    assert_eq!((m.max_units(false), m.max_units(true)), (20, 20));
    assert_eq!(m.apply(merge.clone()), Err(CommandError::Unsupported("the receiving force is full")));
    // The campaign's own value (CAMPAIGN_MODEL #23).
    m.force_caps.army = 21;
    m.apply(merge.clone()).unwrap();
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 21);
    // A new campaign: the original's caps at each `campaign_unit_multiplier` step (20 and 6 / 8 /
    // 10 / 20; the default 0.75 gives 10 ships)...
    let l = crate::limits::GameLimits::default();
    let caps = |m: Option<f32>| super::rules::ForceCaps::new_campaign(&l, m);
    assert_eq!([0.25, 0.5, 0.75, 1.0].map(|s| caps(Some(s))).map(|c| (c.army, c.navy)), [(20, 6), (20, 8), (20, 10), (20, 20)]);
    assert_eq!(caps(None), caps(Some(0.75)));
    // ...and under a mod's 40-unit armies: 25 units arrive, 20 fit.
    let mut m = setup(25);
    Arc::make_mut(&mut m.rules).limits.max_land_units = 40;
    m.force_caps = super::rules::ForceCaps::new_campaign(&m.rules.limits, Some(1.0));
    assert_eq!((m.max_units(false), m.max_units(true)), (40, 20));
    m.apply(merge).unwrap();
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 40);
    assert_eq!(m.world.forces[&ForceId(1005)].units.len(), 5);
}

#[test]
fn command_queue_applies_in_order() {
    let mut m = test_model();
    let mut q = CommandQueue::new();
    q.push(CampaignCommand::DeclareWar { a: A, b: B });
    q.push(CampaignCommand::DeclareWar { a: A, b: B });
    q.push(CampaignCommand::EndTurn);
    assert_eq!(q.len(), 3);
    let results = m.apply_queue(&mut q);
    assert!(q.is_empty());
    assert!(results[0].is_ok());
    assert_eq!(results[1], Err(CommandError::AlreadyAtWar(A, B)));
    assert!(results[2].is_ok());
    assert_eq!(m.calendar.turns_elapsed, 1);
}

/// A fixed, made-up script of commands for the determinism test (A and B are human; each acts
/// in its own turn).
fn play(m: &mut CampaignModel, turns: u32) {
    m.turn.humans = vec![A, B];
    m.start_campaign();
    for t in 0..turns {
        let cur = m.turn.current.unwrap();
        if cur == A && t % 7 == 0 {
            let _ = m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None });
        }
        if cur == A && t % 5 == 0 {
            let _ = m.apply(CampaignCommand::DeclareWar { a: A, b: B });
        }
        if cur == B && t % 5 == 3 {
            let _ = m.apply(CampaignCommand::MakePeace { a: B, b: A });
        }
        if cur == B {
            let _ = m.apply(CampaignCommand::MoveForce { force: ForceId(1001), to: pos((t % 10) as i32, (t % 3) as i32) });
        }
        m.apply(CampaignCommand::EndTurn).unwrap();
    }
}

#[test]
fn identical_models_stay_identical_for_50_turns() {
    let mut a = test_model();
    let mut b = test_model();
    play(&mut a, 50);
    play(&mut b, 50);
    assert_eq!(a, b);
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.world.forces.len() > 2);
    assert_ne!(a.state_hash(), test_model().state_hash());
    let mut c = test_model();
    c.apply(CampaignCommand::SetTaxLevel { faction: C, class: TaxClass::Upper, level: "tax_low".into() }).unwrap();
    play(&mut c, 50);
    assert_ne!(a.state_hash(), c.state_hash());
}

/// Embarking and landing without terrain (the straight-line stand-in of `embark`): the army walks
/// to the navy and boards it, the navy carries it, a move order lands it and landing ends its turn.
#[test]
fn armies_board_navies_sail_and_land() {
    let mut m = test_model();
    let mut admiral = character(103, A, CharacterKind::Admiral, 60);
    admiral.position = pos(2, 0);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1002),
        MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(103)), units: vec![unit(4)], is_navy: true },
    );
    // Only armies board navies of their own faction.
    assert!(m.apply(CampaignCommand::Embark { force: ForceId(1001), navy: ForceId(1002) }).is_err());
    assert!(m.apply(CampaignCommand::Embark { force: ForceId(1002), navy: ForceId(1002) }).is_err());
    let ev = m.apply(CampaignCommand::Embark { force: ForceId(1000), navy: ForceId(1002) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEmbarksNavy { character: CharacterId(100), navy: ForceId(1002) }));
    assert_eq!(m.carrier_of(ForceId(1000)), Some(ForceId(1002)));
    assert_eq!(m.passengers_of(ForceId(1002)), vec![ForceId(1000)]);
    // Boarding spends nothing itself; the walk to the fleet did.
    let general = &m.world.characters[&CharacterId(100)];
    assert_eq!(general.position, pos(2, 0));
    assert!(general.movement_points > 0 && general.movement_points < 30);
    // An embarked army does not board again and may not attack.
    assert!(m.apply(CampaignCommand::Embark { force: ForceId(1000), navy: ForceId(1002) }).is_err());
    assert!(m.apply(CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }).is_err());
    // A second army joins the carried one when both fit into one army (20 units).
    let mut colonel = character(104, A, CharacterKind::Colonel, 25);
    colonel.position = pos(2, 0);
    m.world.characters.insert(colonel.id, colonel);
    m.world.forces.insert(
        ForceId(1003),
        MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(104)), units: vec![unit(5), unit(6)], is_navy: false },
    );
    m.apply(CampaignCommand::Embark { force: ForceId(1003), navy: ForceId(1002) }).unwrap();
    assert!(!m.world.forces.contains_key(&ForceId(1003)), "merged into the carried army");
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 3);
    assert_eq!(m.passengers_of(ForceId(1002)), vec![ForceId(1000)]);
    // The navy sails and carries the army.
    m.apply(CampaignCommand::MoveForce { force: ForceId(1002), to: pos(10, 0) }).unwrap();
    assert_eq!(m.world.characters[&CharacterId(100)].position, pos(10, 0));
    // Next turn (refilled by hand: after end_turn only the current faction may act): a move order
    // to the embarked army lands it; landing ends its turn.
    for c in m.world.characters.values_mut() {
        c.movement_points = c.max_movement_points;
    }
    let ev = m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(14, 0) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterDisembarksNavy { character: CharacterId(100), navy: ForceId(1002) }), "{ev:?}");
    assert_eq!(m.carrier_of(ForceId(1000)), None);
    let general = &m.world.characters[&CharacterId(100)];
    assert_eq!((general.position, general.movement_points), (pos(14, 0), 0));
    // The embark state is part of the state hash.
    let mut a = test_model();
    let b = a.clone();
    a.world.embarked.insert(ForceId(1000), ForceId(1001));
    assert_ne!(a.state_hash(), b.state_hash());
}

/// An army that has already moved this turn cannot land before next turn (CONFIRMED rule:
/// landing costs `ceil(max(g, 1)) + the army's spent fraction`).
#[test]
fn an_army_that_moved_lands_next_turn() {
    let mut m = test_model();
    let mut admiral = character(103, A, CharacterKind::Admiral, 60);
    admiral.position = pos(0, 0);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1002),
        MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(103)), units: vec![unit(4)], is_navy: true },
    );
    m.world.embarked.insert(ForceId(1000), ForceId(1002));
    m.world.characters.get_mut(&CharacterId(100)).unwrap().movement_points = 20; // spent a third
    let ev = m.apply(CampaignCommand::Disembark { force: ForceId(1000), to: pos(3, 0) });
    assert!(matches!(ev, Err(CommandError::NotEnoughMovementPoints { .. })), "{ev:?}");
    assert_eq!(m.carrier_of(ForceId(1000)), Some(ForceId(1002)));
}

#[test]
fn garrison_repression_and_automated_policing() {
    // The shipped caps: policing_garrison_cap 15, policing_automated_cap 4.
    let mut rules = CampaignRules::test_rules();
    rules.variables.insert("policing_garrison_cap".into(), 15.0);
    rules.variables.insert("policing_automated_cap".into(), 4.0);
    // Population factors of 0x008B18F0.
    assert_eq!(economy::garrison_population_factor(5_000), 1.5);
    assert_eq!(economy::garrison_population_factor(9_999), 1.25);
    assert_eq!(economy::garrison_population_factor(100_000), 1.0);
    assert_eq!(economy::garrison_population_factor(500_000), 0.75);
    assert_eq!(economy::garrison_population_factor(2_000_000), 0.5);
    assert_eq!(economy::garrison_population_factor(17_386_000), 0.3);
    // 5 units in a town of 2 million: 2.5 -> 2 (ties to even); 20 units in a village: capped at 15.
    assert_eq!(economy::garrison_repression(&rules, 5, 2_000_000), 2.0);
    assert_eq!(economy::garrison_repression(&rules, 20, 5_000), 15.0);
    assert_eq!(economy::garrison_repression(&rules, 0, 5_000), 0.0);
    // Policing only covers unrest, up to the cap.
    assert_eq!(economy::automated_policing(&rules, 3.0), 0.0);
    assert_eq!(economy::automated_policing(&rules, -3.0), 3.0);
    assert_eq!(economy::automated_policing(&rules, -10.0), 4.0);

    // In the model: region 12 (A, tax_high -> lower -10) gets a 2-unit garrison.
    let mut m = test_model();
    let mut r = (*m.rules).clone();
    r.variables.insert("policing_garrison_cap".into(), 15.0);
    r.variables.insert("policing_automated_cap".into(), 4.0);
    m.rules = Arc::new(r);
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Lower, level: "tax_high".into() }).unwrap();
    m.world.regions.get_mut(&RegionId(12)).unwrap().garrison = Some(ForceId(1000));
    // population 200 000 (test helper: gdp × 100): factor 1.0, 1 unit -> 1; -10 + 1 = -9, policing 4.
    assert_eq!(economy::public_order(&m, RegionId(12)).lower, -10.0 + 1.0 + 4.0);
    assert_eq!(economy::public_order(&m, RegionId(12)).upper, 1.0);
}

#[test]
fn gdp_and_town_wealth_growth_are_recomputed() {
    // Made-up building effects; the formula of 0x00A6AFC0.
    let mut m = test_model_with(tax_growth_mapping().into_iter().chain([map_basic("gdp_mod_all", "gdp_mod_all")]));
    let mut rules = (*m.rules).clone();
    {
        let b = rules.buildings.get_mut("test_building_level").unwrap();
        b.effects.push(("gdp_mine".into(), 300.0));
        b.effects.push(("gdp_farm".into(), 50.0));
        b.effects.push(("tw_growth_industry".into(), 10.0));
        b.effects.push(("tw_growth_port".into(), 4.0));
    }
    // The upper classes' tax level slows growth: modifier -0.3, fixed -3.
    rules.effects.set_tax_bundle(
        "upper_classes",
        "tax_normal",
        mapped(&rules, vec![("tw_growth_taxes_modifier".into(), -0.3), ("tw_growth_taxes_fixed".into(), -3.0)]),
    );
    m.rules = Arc::new(rules);
    let r = m.world.regions[&RegionId(10)].clone();
    // GDP: base 4000 + 300 + 50. Growth: 14 -> trunc(-0.3 × 14) = -4 -> 14 - 4 - 3 = 7.
    assert_eq!(economy::recompute_region(&m, &r), (4350, 7));
    // A damaged building adds nothing; a negative growth is not scaled by the modifier.
    let mut d = r.clone();
    d.slots[0].building.as_mut().unwrap().health = 50;
    assert_eq!(economy::recompute_region(&m, &d), (4000, -3));
    // gdp_mod_all and tw_growth_mod_all scale each building effect (then truncate).
    let mut rules = (*m.rules).clone();
    rules.government_effects.insert(String::new(), vec![("gdp_mod_all".into(), 10.0), ("tw_growth_mod_all".into(), 50.0)]);
    m.rules = Arc::new(rules);
    // 300 × 1.1 = 330, 50 × 1.1 = 55; growth 10 × 1.5 = 15, 4 × 1.5 = 6 -> 21 - 6 - 3 = 12.
    assert_eq!(economy::recompute_region(&m, &r), (4385, 12));
    // A faction-wide effect (a researched technology with gdp_mod_all +20) counts at the round end
    // (recompute_region_with), not in the start-position form: 300 × 1.3 = 390, 50 × 1.3 = 65.
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("gdp_mod_all", 20.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("test_tech".into(), 0)];
    let fx = super::effects::Effects::compute(&m);
    assert_eq!(economy::recompute_region(&m, &r), (4385, 12));
    assert_eq!(economy::recompute_region_with(&m, Some(&fx), &r), (4455, 12));
}

#[test]
fn trade_routes_from_agreements() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_value_accumulator_proportion".into(), 0.005);
    m.rules = Arc::new(rules);
    assert_eq!(economy::trade_routes_value(&m, A), 0);
    // A (GDP 100 + 4000 + 2000) and B (100 + 8000) agree to trade: each earns
    // trunc(2.2 × sqrt(14200)) = 262 a turn.
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    assert_eq!(economy::faction_gdp(&m, A), 6100);
    assert_eq!(economy::trade_routes_value(&m, A), 262);
    assert_eq!(economy::trade_routes_value(&m, B), 262);
    assert_eq!(economy::faction_income(&m, A).trade, 262);
    // The round end accumulates trunc(0.005 × 262) = 1 per route.
    economy::accumulate_trade(&mut m, A);
    assert_eq!(economy::trade_routes_value(&m, A), 263);
    // War ends the route.
    m.world.set_stance(A, B, Stance::War).unwrap();
    assert_eq!(economy::trade_routes_value(&m, A), 0);
}

#[test]
fn trade_commodities_and_blockades() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_value_accumulator_proportion".into(), 0.005);
    m.rules = Arc::new(rules);
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    // A ships 2 of commodity 0 (price 10) and 1 of commodity 1 (price 20) to B by sea, from its
    // port at (20, 0): commodity part 40 on top of the GDP part 262.
    m.world.commodity_prices = vec![10, 20];
    let port = pos(20, 0);
    let path = TradePath {
        waypoints: vec![
            TradeWaypoint { region: RegionId(10), from: 1, to: 2, sea: true, from_pos: Some(port), to_pos: Some(pos(90, 0)) },
            TradeWaypoint { region: RegionId(11), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None },
        ],
        volumes: vec![2, 1],
    };
    m.world.trade_paths.insert((A, B), vec![path.clone()]);
    assert_eq!(economy::trade_routes_value(&m, A), 302);
    // B has no path loaded: GDP part only.
    assert_eq!(economy::trade_routes_value(&m, B), 262);
    // Accumulation counts the commodity part: trunc(0.005 × 302) = 1.
    economy::accumulate_trade(&mut m, A);
    assert_eq!(m.world.trade_accumulated[&(A, B)], 1);
    // A navy of C (at war with A) at the port blockades the route: nothing, not even the
    // accumulated value, and nothing accumulates.
    let mut admiral = character(104, C, CharacterKind::Admiral, 60);
    admiral.position = pos(20, 0);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1003),
        MilitaryForce { id: ForceId(1003), faction: C, commander: Some(CharacterId(104)), units: vec![unit(5)], is_navy: true },
    );
    assert_eq!(economy::trade_routes_value(&m, A), 303, "not at war: no blockade");
    m.world.set_stance(A, C, Stance::War).unwrap();
    assert!(m.trade_path_blockaded(A, &path));
    assert_eq!(economy::trade_routes_value(&m, A), 0);
    economy::accumulate_trade(&mut m, A);
    assert_eq!(m.world.trade_accumulated[&(A, B)], 1);
    // Far from the port it no longer blocks.
    m.world.characters.get_mut(&CharacterId(104)).unwrap().position = pos(50, 30);
    assert_eq!(economy::trade_routes_value(&m, A), 303);
    // With a domestic route the supply is split (`0x00BC26D0`): B has no demand in this world, so it takes no
    // share and the route carries nothing (the loaded volumes are replaced); a blockaded domestic route
    // cuts the supply to 0 as well.
    let home = TradePath {
        waypoints: vec![TradeWaypoint { region: RegionId(0), from: 120, to: 1, sea: true, from_pos: Some(pos(60, 30)), to_pos: Some(port) }],
        volumes: vec![2, 1],
    };
    m.world.domestic_trade.insert(A, vec![home]);
    assert_eq!(economy::trade_routes_value(&m, A), 263);
    m.world.characters.get_mut(&CharacterId(104)).unwrap().position = pos(60, 30);
    assert_eq!(m.trade_supply(A), Some(vec![0, 0]));
    assert_eq!(economy::trade_routes_value(&m, A), 263);
}

#[test]
fn commanders_get_the_force_action_point_factor() {
    let mut rules = CampaignRules::test_rules();
    assert_eq!(turn::force_action_point_factor(&rules, 0.0), 1.0);
    rules.variables.insert("general_admiral_action_point_bonus".into(), 0.2);
    assert_eq!(turn::force_action_point_factor(&rules, 0.0), 1.2);
    rules.variables.insert("general_admiral_action_point_bonus".into(), 0.9);
    assert_eq!(turn::force_action_point_factor(&rules, 0.0), 1.6);
    rules.variables.insert("general_admiral_action_point_bonus".into(), 0.2);
    let mut m = test_model();
    m.rules = Arc::new(rules);
    m.turn.humans = vec![A];
    m.start_campaign();
    // Character 100 commands force 1000: 30 × 1.2 = 36. Character 102 (a minister) does not.
    assert_eq!(m.world.characters[&CharacterId(100)].movement_points, 36);
    assert_eq!(m.world.characters[&CharacterId(102)].movement_points, 0);
}

#[test]
fn new_trade_agreements_get_a_route_over_the_network() {
    use super::trade::TradeLeg;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    // The test building gives a sea route cap (`trade_routes_mod_max_sea`, 0x008DC150).
    m.world.commodity_prices = vec![10];
    rules.buildings.get_mut("test_building_level").unwrap().effects.push(("trade_routes_mod_max_sea".into(), 3.0));
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_land_sea_bias".into(), 0.5);
    m.rules = Arc::new(rules);
    m.world.commodity_prices = vec![10];
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    // Nodes: 1 = settlement of region 10 (A), 2 = A's port in region 10, 3 = B's port in region 11,
    // 4 = settlement of region 11 (B); 5 = a settlement of C in between.
    m.world.trade_node_regions =
        [(1, (RegionId(10), false)), (2, (RegionId(10), true)), (3, (RegionId(11), true)), (4, (RegionId(11), false))].into_iter().collect();
    // Land 1-4 costs 100; land 1-2 (5) + sea 2-3 (100 × 0.5) + land 3-4 (5) costs 60.
    m.world.trade_network = vec![
        TradeLeg { from: 1, to: 4, length: 100.0 },
        TradeLeg { from: 1, to: 2, length: 5.0 },
        TradeLeg { from: 2, to: 3, length: 100.0 },
        TradeLeg { from: 3, to: 4, length: 5.0 },
    ];
    let route = m.build_trade_route(A, B).expect("route");
    let hops: Vec<(u32, u32, bool)> = route.waypoints.iter().map(|w| (w.from, w.to, w.sea)).collect();
    // A's port is a source itself, so the route starts there and ends at B's port.
    assert_eq!(hops, vec![(2, 3, true), (u32::MAX, u32::MAX, false)]);
    assert_eq!(route.waypoints.last().unwrap().region, RegionId(11));
    assert_eq!(economy::trade_routes_value(&m, A), 262);
    // With a sea length limit below 100 only the land route is left.
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_internat_sea_length_limit".into(), 50.0);
    m.rules = Arc::new(rules);
    let hops: Vec<(u32, u32)> = m.build_trade_route(A, B).unwrap().waypoints.iter().map(|w| (w.from, w.to)).collect();
    assert_eq!(hops[0], (1, 4));
    // Without a sea cap no sea leg is allowed either.
    let mut rules = (*m.rules).clone();
    rules.variables.remove("trade_route_internat_sea_length_limit");
    rules.buildings.get_mut("test_building_level").unwrap().effects.retain(|(k, _)| k != "trade_routes_mod_max_sea");
    m.rules = Arc::new(rules);
    let hops: Vec<(u32, u32)> = m.build_trade_route(A, B).unwrap().waypoints.iter().map(|w| (w.from, w.to)).collect();
    assert_eq!(hops[0], (1, 4));
    // No path at all: no route, no income from the pair.
    m.world.trade_network.clear();
    m.world.trade_network.push(TradeLeg { from: 7, to: 8, length: 1.0 });
    assert!(m.build_trade_route(A, B).is_none());
    assert_eq!(economy::trade_routes_value(&m, A), 0);
}

#[test]
fn trade_fleets_gather_commodities_at_nodes() {
    use super::trade::{node_volume, TradeNode, TradeNodeInfo};
    let info = TradeNodeInfo { commodity: 0, base: 20, per_ship: 0.85, cap: 50.0 };
    // 0x00BC9930: trunc((min((n − 1) × 0.85, 50) + 1) × 20).
    assert_eq!([0, 1, 2, 3, 4].map(|n| node_volume(&info, n)), [0, 20, 37, 54, 71]);
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    let mut ship = rules.units["test_unit"].clone();
    ship.category = "naval_merchant".into();
    ship.is_naval = true;
    rules.units.insert("trade_ship".into(), ship);
    m.rules = Arc::new(rules);
    m.world.commodity_prices = vec![10];
    m.world.trade_nodes = vec![TradeNode { node: 120, pos: (40.0, 0.0), info: Some(info) }];
    let mut admiral = character(105, A, CharacterKind::Admiral, 60);
    admiral.position = pos(40, 0);
    m.world.characters.insert(admiral.id, admiral);
    let mut ships = vec![unit(6), unit(7)];
    for u in &mut ships {
        u.unit_key = "trade_ship".into();
    }
    m.world.forces.insert(ForceId(1004), MilitaryForce { id: ForceId(1004), faction: A, commander: Some(CharacterId(105)), units: ships, is_navy: true });
    assert_eq!(m.trade_supply(A), Some(vec![37]));
    // Sailing away ends the gathering.
    m.world.characters.get_mut(&CharacterId(105)).unwrap().position = pos(42, 0);
    assert_eq!(m.trade_supply(A), Some(vec![0]));
}

#[test]
fn religion_gentlemen_war_and_education_in_public_order() {
    let mut m = test_model_with([map_basic("gentleman_happiness_bonus", "gentleman_happiness_bonus")]);
    let base = economy::public_order(&m, RegionId(10));
    // Religion: a protestant faction ruling a region 70 % catholic, unrest factor 0.2 for (catholic,
    // protestant): −round(0.7 × 100 × 0.2) = −14.
    let mut rules = (*m.rules).clone();
    rules.religion_relations.insert(("rel_catholic".into(), "rel_protestant".into()), 0.2);
    rules.variables.insert("gentleman_happiness_divisor".into(), 3.0);
    rules.variables.insert("gentleman_happiness_positive_limit".into(), 6.0);
    rules.variables.insert("gentleman_happiness_negative_limit".into(), -8.0);
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().religion = "rel_protestant".into();
    m.world.regions.get_mut(&RegionId(10)).unwrap().religions = vec![("rel_catholic".into(), 0.7), ("rel_protestant".into(), 0.3)];
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower - 14.0);
    m.world.regions.get_mut(&RegionId(10)).unwrap().religions.clear();
    // Gentlemen: the skill term is 0; each gentleman gives his faction's gentleman_happiness_bonus,
    // own ones positive, foreign ones negative.
    let mut rules = (*m.rules).clone();
    for (key, v) in [("tech_gent_a", 1.0), ("tech_gent_c", 2.0)] {
        rules.effects.insert_technology(key.into(), mapped(&rules, vec![("gentleman_happiness_bonus", v)]));
    }
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("tech_gent_a".into(), 0)];
    m.world.faction_details.entry(C).or_default().technologies = vec![("tech_gent_c".into(), 0)];
    let mut g = character(106, A, CharacterKind::Gentleman, 10);
    g.position = pos(10, 0);
    m.world.characters.insert(g.id, g);
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower + 1.0);
    let mut g = character(107, C, CharacterKind::Gentleman, 10);
    g.position = pos(11, 0);
    m.world.characters.insert(g.id, g);
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower + 1.0 - 2.0);
    m.world.characters.remove(&CharacterId(106));
    m.world.characters.remove(&CharacterId(107));
    m.world.faction_details.entry(A).or_default().technologies.clear();
    m.world.faction_details.entry(C).or_default().technologies.clear();
    // War results: a stored base counts as it is; with base 0 an enemy army in the region adds 12.
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = vec![("lower".into(), 0, 0), ("upper".into(), 0, 3)];
    assert_eq!(economy::public_order(&m, RegionId(10)).upper, base.upper + 3.0);
    m.world.set_stance(A, C, Stance::War).unwrap();
    let mut col = character(108, C, CharacterKind::Colonel, 10);
    col.position = pos(12, 0);
    m.world.characters.insert(col.id, col);
    // A character without a force does not count.
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower);
    let army = MilitaryForce { id: ForceId(1008), faction: C, commander: Some(CharacterId(108)), units: vec![unit(8)], is_navy: true };
    m.world.forces.insert(army.id, army);
    // Nor does a fleet.
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower);
    m.world.forces.get_mut(&ForceId(1008)).unwrap().is_navy = false;
    let po = economy::public_order(&m, RegionId(10));
    assert_eq!((po.lower, po.upper), (base.lower + 12.0, base.upper + 3.0));
}

#[test]
fn the_winner_goes_on_into_the_settlement_after_beating_the_army_outside() {
    let mut m = test_model();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    // B's army stands outside its settlement (region 11 at (11, 0)); A's strong army comes to take it.
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(12, 0);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(15, 0);
    let mut units = Vec::new();
    for i in 0..6 {
        let mut u = unit(20 + i);
        u.men = 100;
        units.push(u);
    }
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = units;
    for u in &mut m.world.forces.get_mut(&ForceId(1001)).unwrap().units {
        u.men = 20;
    }
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::PreBattle { attacker: ForceId(1000) }));
    let pb = m.pending_battle.clone().unwrap();
    assert_eq!((pb.defenders.clone(), pb.settlement, pb.resume), (vec![ForceId(1001)], None, Some(RegionId(11))));
    let ev = m.apply(CampaignCommand::Autoresolve).unwrap();
    assert!(matches!(ev[0], CampaignEvent::BattleCompleted { attacker_won: true, .. }));
    // The settlement had no garrison: the winner walks in and takes it.
    assert!(ev.contains(&CampaignEvent::SettlementOccupied { region: RegionId(11), faction: A }));
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
}

#[test]
fn faction_wide_effects_reach_public_order() {
    let mut m = test_model_with([
        ("happy_character_lower".to_string(), vec![EffectKey { kind: BonusKind::PopClass, bonus: "happiness_character".into(), qualifier: "lower".into() }]),
        map_basic("repression_ministers", "repression_ministers"),
    ]);
    let base = economy::public_order(&m, RegionId(10));
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("happy_character_lower", 3.0), ("repression_ministers", 2.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("test_tech".into(), 0)];
    let po = economy::public_order(&m, RegionId(10));
    assert_eq!((po.lower, po.upper), (base.lower + 5.0, base.upper + 2.0));
}

#[test]
fn sea_route_cap_walks_owned_land_neighbours_from_the_capital() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.get_mut("test_building_level").unwrap().effects.push(("trade_routes_mod_max_sea".into(), 3.0));
    m.rules = Arc::new(rules);
    // A owns regions 10 (capital) and 12, each with one port-like building (cap 3); B owns 11.
    let mut slot = m.world.regions[&RegionId(10)].slots[0].clone();
    slot.key = "test_slot_12".into();
    m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(slot);
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    // Without the map's neighbours every owned region counts.
    assert_eq!(m.sea_route_cap(A), 6);
    // 12 is reached only through B's region 11: the walk stops there.
    m.world.region_neighbours =
        [(RegionId(10), vec![RegionId(11)]), (RegionId(11), vec![RegionId(10), RegionId(12)]), (RegionId(12), vec![RegionId(11)])].into_iter().collect();
    assert_eq!(m.sea_route_cap(A), 3);
    // A land link from the capital to 12.
    m.world.region_neighbours.get_mut(&RegionId(10)).unwrap().push(RegionId(12));
    m.world.region_neighbours.get_mut(&RegionId(12)).unwrap().push(RegionId(10));
    assert_eq!(m.sea_route_cap(A), 6);
    // No capital: no cap.
    m.world.faction_details.entry(A).or_default().capital = None;
    assert_eq!(m.sea_route_cap(A), 0);
}

#[test]
fn compute_for_gives_the_same_answers_for_its_faction() {
    use super::effects::Effects;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("recruitment_points", 2.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("test_tech".into(), 0)];
    let full = Effects::compute(&m);
    for f in [A, B] {
        let part = Effects::compute_for(&m, f);
        assert_eq!(part.faction.get(&f), full.faction.get(&f));
        for r in m.world.regions.values().filter(|r| r.owner == f) {
            assert_eq!(part.region_local.get(&r.id), full.region_local.get(&r.id));
            for key in ["recruitment_points", "happiness_active_lower_tax"] {
                assert_eq!(part.region(r.id, key), full.region(r.id, key));
            }
            assert_eq!(m.recruitment_points_with(&part, r.id, false), m.recruitment_points_with(&full, r.id, false));
        }
        for c in m.world.characters.values().filter(|c| c.faction == f) {
            assert_eq!(part.character_total(c.id, "recruitment_points"), full.character_total(c.id, "recruitment_points"));
        }
        assert_eq!(economy::faction_income_with(&m, &part, f), economy::faction_income_with(&m, &full, f));
    }
}

/// A test model for the trade split: A trades with B (region 11, GDP 8000) and C (region 12, given to
/// C, GDP 2000); A's capital is region 10 and its trade ships bring home `home` of each commodity.
fn split_model(commodities: &[&str], home: Vec<u32>) -> CampaignModel {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    // Demand of `c0` = the region's GDP; of `c1` = trunc(sqrt(town wealth)).
    rules.commodity_demand.push(("c0".into(), "ddr_GDP".into(), 1.0, 1.0));
    rules.commodity_demand.push(("c1".into(), "ddr_TW".into(), 1.0, 1.0));
    m.rules = Arc::new(rules);
    m.world.commodity_keys = commodities.iter().map(|c| c.to_string()).collect();
    m.world.commodity_prices = vec![10; commodities.len()];
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    for b in [B, C] {
        for (x, y) in [(A, b), (b, A)] {
            m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
        }
        let end = TradeWaypoint { region: RegionId(11), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None };
        m.world.trade_paths.insert((A, b), vec![TradePath { waypoints: vec![end], volumes: vec![0; commodities.len()] }]);
    }
    let w = TradeWaypoint { region: RegionId(10), from: 120, to: 1, sea: true, from_pos: None, to_pos: None };
    m.world.domestic_trade.insert(A, vec![TradePath { waypoints: vec![w], volumes: home }]);
    m
}

/// `0x00BC26D0` (CONFIRMED): per commodity the partners, sorted by net demand, take
/// `max(1, demand × rest / total)` from the smallest demand up while there are at least as many units
/// left as partners.
#[test]
fn supply_is_split_over_the_partners_by_their_net_demand() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = split_model(&["c0"], vec![33]);
    // The smaller demand first: C trunc(33 × 2000 / 10000) = 6, then B the rest, 27.
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C].clone(), split[&B].clone()), (vec![6], vec![27]));
    // A partner's own supply comes off its demand when it has a capital: B brings home 6000 itself,
    // so both partners have 2000. Equal demands keep the faction list order (B before C: turn order
    // B, A, C) and are handed out from the end: C 33 × 2000 / 4000 = 16 first, then B the rest, 17.
    let w = TradeWaypoint { region: RegionId(11), from: 121, to: 2, sea: true, from_pos: None, to_pos: None };
    m.world.domestic_trade.insert(B, vec![TradePath { waypoints: vec![w], volumes: vec![6000] }]);
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C].clone(), split[&B].clone()), (vec![16], vec![17]));
    // Without its capital B's own supply does not count.
    m.world.faction_details.entry(B).or_default().capital = None;
    assert_eq!(m.trade_split(A).unwrap()[&B], vec![27]);
    // A second route of the pair carries the same share (the exe adds it to every route of the pair).
    let path = m.world.trade_paths[&(A, B)][0].clone();
    m.world.trade_paths.get_mut(&(A, B)).unwrap().push(path.clone());
    let split = m.trade_split(A).unwrap();
    for p in &m.world.trade_paths[&(A, B)] {
        assert_eq!(m.trade_path_volumes(Some(&split), B, p), vec![27]);
    }
    // An exporter without a capital exports nothing.
    m.world.faction_details.entry(A).or_default().capital = None;
    assert_eq!(m.trade_split(A).unwrap().values().cloned().collect::<Vec<_>>(), vec![vec![0], vec![0]]);
}

#[test]
fn partners_without_net_demand_take_nothing_and_the_rest_may_stay_home() {
    // No demand at all: no partner takes a share, the supply is not exported.
    let mut m = split_model(&["c0"], vec![33]);
    let mut rules = (*m.rules).clone();
    rules.commodity_demand.clear();
    m.rules = Arc::new(rules);
    assert_eq!(m.trade_split(A).unwrap().values().cloned().collect::<Vec<_>>(), vec![vec![0], vec![0]]);

    // C has no demand for c0 (GDP 0) but some for c1 (town wealth 100 → 10), so it counts with demand
    // 0 for c0: the total is 8000 + max(0, 1) = 8001. Of 5 units C gets max(1, 0) = 1, then B
    // trunc(8000 × 4 / 8001) = 3; the last unit is not exported.
    let mut m = split_model(&["c0", "c1"], vec![5, 0]);
    let r = m.world.regions.get_mut(&RegionId(12)).unwrap();
    (r.gdp, r.town_wealth) = (0, 100);
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C][0], split[&B][0]), (1, 3));
    // One unit for two partners: C (first, two partners left) gets nothing, B takes max(1, 0) = 1.
    m.world.domestic_trade.get_mut(&A).unwrap()[0].volumes = vec![1, 0];
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C][0], split[&B][0]), (0, 1));
}

#[test]
fn construction_cost_takes_the_local_and_chain_cost_effects() {
    use super::effects::SavedBonus;
    let mut m = test_model_with([map_chain_cost("building_cost_mod_all", "test_chain")]);
    // Region 10 holds a test building (level 0) at full health; the upgrade costs 600.
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 600);
    // A timber-like local effect, mapped to the chain through the chain junction: −10 %.
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_all", -10.0)]));
    m.rules = Arc::new(rules);
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 540);
    // The owner's chain-keyed entry (saved type 2, id 0) adds up: −10 − 50 = −60 %.
    m.world.faction_details.entry(A).or_default().bonus_base =
        vec![SavedBonus { kind: 2, bonus: 0, value: -50.0, qualifier: "test_chain".into() }];
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 240);
    // A slot held by another faction drops the local effect.
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].holder = Some(B);
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 300);
    assert_eq!(m.can_build(RegionId(10), SlotRef::Slot(0), "test_building_level_2"), Ok(300));
}

#[test]
fn construction_cost_counts_only_the_chains_an_effect_maps_to() {
    // `building_cost_mod_all` maps to 19 vanilla chains, not to every chain (e.g. not `rHorse`): a local effect
    // mapped to another chain leaves this one at full cost.
    let mut m = test_model_with([map_chain_cost("building_cost_mod_other_chain", "other_chain")]);
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_other_chain", -10.0)]));
    m.rules = Arc::new(rules);
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 600);
}

#[test]
fn construction_cost_takes_the_faction_wide_and_technology_entries() {
    use super::effects::TECH_RESEARCHED;
    let mut m = test_model_with([map_chain_cost("building_cost_mod_all_global", "test_chain"), map_chain_cost("building_cost_mod_industry", "test_chain")]);
    let mut rules = (*m.rules).clone();
    // A faction-wide building effect (the steam sawmill's −2 %) reaches every region of the owner.
    rules.effects.insert_building_factionwide("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_all_global", -2.0)]));
    // A researched technology's chain cost effect (`economy2_joint_stock_company`: −10 % on factories).
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("building_cost_mod_industry", -10.0)]));
    m.rules = Arc::new(rules);
    // 600 × 98 × 0.01 = 588.
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 588);
    m.world.faction_details.entry(A).or_default().technologies.push(("test_tech".into(), TECH_RESEARCHED));
    // 600 × 88 × 0.01 = 528.
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 528);
    // The construction options and the repair read the same modifier.
    assert_eq!(m.building_cost_modifier(RegionId(10), "test_building_level_2"), -12.0);
    let opts = m.construction_options(RegionId(10), SlotRef::Slot(0));
    assert_eq!(opts.iter().find(|o| o.level_key == "test_building_level_2").map(|o| o.cost), Some(528));
}

#[test]
fn construction_cost_rounds_half_to_even_in_f32() {
    // `cost × (modifier + 100) × 0.01` in f32, then FISTP: 5 × 50 × 0.01 = 2.5 → 2, 3 × 50 × 0.01 = 1.5 → 2.
    assert_eq!(super::commands::building_cost(5, -50.0), 2);
    assert_eq!(super::commands::building_cost(3, -50.0), 2);
    assert_eq!(super::commands::building_cost(8000, -58.0), 3360);
    // No clamp: below −100 the cost is negative.
    assert_eq!(super::commands::building_cost(1000, -110.0), -100);
    // Out of the int range FISTP stores the integer indefinite 0x80000000.
    assert_eq!(super::commands::building_cost(2_000_000_000, 100.0), i32::MIN);
    assert_eq!(super::commands::building_cost(1000, f32::NAN), i32::MIN);
}

/// The tax-level bundles are compiled when set (through the rules' fixed mapping), not on every region
/// effect set; the region's set merges the compiled bundle of its owner's level.
#[test]
fn tax_bundles_are_compiled_once_and_merged_into_the_region_set() {
    let mut m = test_model();
    let rules = CampaignRules::test_rules();
    assert_eq!(rules.effects.tax_set("lower_classes", "tax_high").unwrap().get_int("happiness_active_lower_tax"), -10);
    assert_eq!(rules.effects.tax_rows("lower_classes", "tax_high"), Some(&vec![("happy_active_lower_tax".to_string(), -10.0)]));
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().tax_lower = "tax_high".into();
    let r = m.world.regions[&RegionId(10)].clone();
    assert_eq!(economy::region_effect_set(&m, &r).get_int("happiness_active_lower_tax"), -10);
}

#[test]
fn a_negative_cost_shows_unaffordable_and_is_paid_out_only_from_a_negative_treasury() {
    let mut m = test_model_with([map_chain_cost("building_cost_mod_all", "test_chain")]);
    m.turn.humans = vec![A];
    m.start_campaign();
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_all", -150.0)]));
    m.rules = Arc::new(rules);
    // Slot 0 holds the test building; its upgrade (600) now costs 600 × −50 × 0.01 = −300.
    let upgrade = |m: &CampaignModel| m.construction_options(RegionId(10), SlotRef::Slot(0)).into_iter().find(|o| o.level_key == "test_building_level_2").unwrap();
    let cmd = CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level_2".into() };
    m.world.factions.get_mut(&A).unwrap().treasury = 1000;
    let o = upgrade(&m);
    assert_eq!((o.cost, o.affordable), (-300, false), "unsigned: 0xFFFFFED4 > 1000");
    // The command's list drops the option (`0x00B43300` with flag 1 set): refused, nothing paid.
    assert_eq!(m.apply(cmd.clone()), Err(CommandError::NegativeCost { cost: -300, available: 1000 }));
    assert_eq!(m.world.factions[&A].treasury, 1000);
    // A treasury of −5 is 0xFFFFFFFB unsigned, not below the cost: listed, and −5 ≥ −300 signed, so it pays out.
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    assert!(upgrade(&m).affordable);
    m.apply(cmd).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 295);
}

/// Review: `NegativeCost` is only the unsigned test's refusal of a negative cost. A positive cost passes the
/// unsigned test against a negative treasury (0xFFFFFFFB) and is refused by the signed one: a plain shortfall.
#[test]
fn the_signed_treasury_refusal_is_insufficient_funds() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    let cmd = CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level_2".into() };
    let cost = m.can_build(RegionId(10), SlotRef::Slot(0), "test_building_level_2").unwrap();
    assert!(cost > 0 && super::treasury::construction_affordable(-5, cost));
    assert_eq!(m.apply(cmd), Err(CommandError::InsufficientFunds { needed: cost, available: -5 }));
    assert_eq!(m.world.factions[&A].treasury, -5);
}

#[test]
fn an_undamaged_building_costs_nothing_to_repair_whatever_the_modifier() {
    use super::effects::SavedBonus;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().bonus_base =
        vec![SavedBonus { kind: 2, bonus: 0, value: 50.0, qualifier: "test_chain".into() }];
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Slot(0)), 0);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
    // 300 × 50 × 0.01 = 150, × 150 × 0.01 = 225.
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Slot(0)), 225);
    // The level cost is read unsigned (0xFFFFFFFF) and an out-of-range product is FISTP's 0x80000000, as in the
    // construction cost.
    let mut rules = (*m.rules).clone();
    rules.buildings.get_mut("test_building_level").unwrap().cost = -1;
    m.rules = Arc::new(rules);
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Slot(0)), i32::MIN);
}

#[test]
fn a_repair_costing_i32_min_is_queued_and_charges_nothing() {
    // Cost 0xFFFFFFFF, health 50: the repair cost is FISTP's 0x80000000. `0x00B16430` lets it through (cost ≤
    // treasury, signed) and `0x00B66260` charges only a cost above 0, so the repair is queued for free.
    let setup = |human: bool| {
        let mut m = test_model();
        let humans = if human { vec![A] } else { Vec::new() };
        m.turn = TurnState::in_turn_of(A, humans);
        let mut rules = (*m.rules).clone();
        rules.buildings.get_mut("test_building_level").unwrap().cost = -1;
        m.rules = Arc::new(rules);
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
        m.world.factions.get_mut(&A).unwrap().treasury = 1000;
        m
    };
    let mut m = setup(true);
    assert_eq!(m.repair_cost(RegionId(10), SlotRef::Slot(0)), i32::MIN);
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 1000);
    assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| (c.slot, c.cost)), Some((SlotRef::Slot(0), i32::MIN)));
    // Cancelling credits the stored cost back whatever was charged (`0x00B1A790`), wrapping as the exe's add.
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 1000i32.wrapping_add(i32::MIN));
    // The AI's repair goes the same way.
    let mut ai = setup(false);
    ai.ai_repairs(A);
    assert_eq!(ai.world.factions[&A].treasury, 1000);
    assert_eq!(ai.world.regions[&RegionId(10)].construction.last().map(|c| c.slot), Some(SlotRef::Slot(0)));
}

#[test]
fn an_ai_repair_is_capped_at_the_treasury_and_free_in_debt() {
    let ai = |treasury: i32| {
        let mut m = test_model();
        m.turn.humans = Vec::new();
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
        m.world.factions.get_mut(&A).unwrap().treasury = treasury;
        m.ai_repairs(A);
        assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| c.slot), Some(SlotRef::Slot(0)));
        m.world.factions[&A].treasury
    };
    // Full cost 150; the AI's cost is capped at its treasury (`0x00B66410`), so it always passes `0x00B16430`.
    assert_eq!(ai(100), 0);
    assert_eq!(ai(1000), 850);
    // In debt the capped cost is negative and `0x00B66260` charges nothing.
    assert_eq!(ai(-20), -20);
}

#[test]
fn cancelling_a_repair_credits_its_stored_cost() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
    // A human repair with the treasury below its cost: `0x00B66260` tests no funds and charges 150.
    m.world.factions.get_mut(&A).unwrap().treasury = 100;
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, -50);
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 100);
    // An AI repair queued in debt charged nothing but stores its capped cost −20; a cancel credits −20 (the exe's
    // `0x00B1A790` credits the stored cost, not what was charged).
    m.turn = TurnState::in_turn_of(A, Vec::new());
    m.world.factions.get_mut(&A).unwrap().treasury = -20;
    m.ai_repairs(A);
    assert_eq!(m.world.factions[&A].treasury, -20);
    assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| c.cost), Some(-20));
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, -40);
}

#[test]
fn region_options_build_the_same_lists_as_each_slot() {
    let m = test_model();
    let new_levels = m.new_levels_by_slot_type();
    let all = m.region_construction_options(RegionId(10), Some(&new_levels));
    assert!(all.iter().any(|(_, o)| !o.is_empty()));
    for (slot, options) in all {
        assert_eq!(options, m.construction_options_in(RegionId(10), slot, Some(&new_levels)), "{slot:?}");
    }
}

#[test]
fn a_treasury_below_zero_shows_every_card_affordable_but_the_command_refuses() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    // The panel's flag compares unsigned (`0x00B43300`).
    let opts = m.construction_options(RegionId(10), SlotRef::Slot(1));
    assert!(!opts.is_empty() && opts.iter().all(|o| o.affordable));
    // The construct command compares signed (`0x00B13DD0`).
    assert!(matches!(
        m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level".into() }),
        Err(CommandError::InsufficientFunds { .. })
    ));
}

#[test]
fn a_dead_commanders_force_goes_to_a_new_colonel() {
    let mut m = test_model();
    // Force 1001 (B, two units) loses its commander 101; force 1000's commander 100 dies with an
    // empty force after its only unit is removed.
    let pos101 = m.world.characters[&CharacterId(101)].position;
    m.character_dies(CharacterId(101));
    assert!(!m.world.characters.contains_key(&CharacterId(101)));
    let f = &m.world.forces[&ForceId(1001)];
    let colonel = f.commander.expect("a new commander");
    assert_ne!(colonel, CharacterId(101));
    assert_eq!(f.units[0].character, Some(colonel));
    let c = &m.world.characters[&colonel];
    assert_eq!((c.kind, c.faction, c.position), (CharacterKind::Colonel, B, pos101));
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.clear();
    m.character_dies(CharacterId(100));
    assert!(!m.world.forces.contains_key(&ForceId(1000)));
    assert!(m.world.forces.values().all(|f| f.commander.is_some_and(|c| m.world.characters.contains_key(&c))));
}

#[test]
fn schools_research_technologies_and_unlock_the_next() {
    use super::research::{state, ResearchError};
    use super::rules::TechRules;
    let mut m = test_model_with([map_basic("research_points", "research_points")]);
    let mut rules = (*m.rules).clone();
    // The test building is a school: research_points 10.
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("research_points", 10.0)]));
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![] });
    rules.technologies.insert("admin2_b".into(), TechRules { cost: 30, building_level: "test_building_level".into(), requires: vec!["admin1_a".into()] });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin1_a".into(), state::AVAILABLE), ("admin2_b".into(), state::UNAVAILABLE)];
    let school_pos = pos(10, 2);
    {
        let s = &mut m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0];
        s.id = 77;
        s.position = Some(school_pos);
    }
    // A gentleman with research 2 in the school adds 2 + 1.
    let mut g = character(120, A, CharacterKind::Gentleman, 10);
    g.position = school_pos;
    m.world.characters.insert(g.id, g);
    m.world.character_details.entry(CharacterId(120)).or_default().attributes = vec![("research".into(), 2)];
    assert_eq!(m.research_rate(RegionId(10), 0, "admin1_a"), 13.0);
    // Not a school, not available yet.
    assert_eq!(m.start_research(RegionId(10), 1, "admin1_a"), Err(ResearchError::NotASchool));
    assert_eq!(m.start_research(RegionId(10), 0, "admin2_b"), Err(ResearchError::NotAvailable));
    m.start_research(RegionId(10), 0, "admin1_a").unwrap();
    assert_eq!(m.researching_at(RegionId(10), 0).as_deref(), Some("admin1_a"));
    assert!(m.research_step(A).is_empty());
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].progress, 13.0);
    // The second step reaches the cost: researched, progress = cost, school free, the next tech opens.
    assert_eq!(m.research_step(A), vec!["admin1_a".to_string()]);
    let d = &m.world.faction_details[&A];
    assert_eq!(d.research["admin1_a"].progress, 25.0);
    assert_eq!(d.research["admin1_a"].researcher, 0);
    assert_eq!(m.tech_state(A, "admin1_a"), Some(state::RESEARCHED));
    assert_eq!(m.tech_state(A, "admin2_b"), Some(state::AVAILABLE));
    assert!(m.researching_at(RegionId(10), 0).is_none());
    // A finished technology cannot be researched again.
    assert_eq!(m.start_research(RegionId(10), 0, "admin1_a"), Err(ResearchError::NotAvailable));
}

/// Unit rules for the commander pick tests (MADE-UP keys; categories are the DB's).
fn with_pick_units(m: &mut CampaignModel) {
    let mut rules = (*m.rules).clone();
    for (k, category, campaign_cost, flag_21, is_naval) in [
        ("t_cav", "cavalry", 0, false, false),
        ("t_art", "artillery", 0, false, false),
        ("t_inf_rich", "infantry", 900, false, false),
        ("t_inf_flag", "infantry", 900, true, false),
        ("t_line", "naval_line_of_battle", 0, false, true),
        ("t_frig", "naval_frigate", 0, false, true),
    ] {
        rules.units.insert(k.into(), UnitRules { category: category.into(), campaign_cost, flag_21, is_naval, ..Default::default() });
    }
    m.rules = Arc::new(rules);
}

fn keyed(id: i32, key: &str) -> CampaignUnit {
    CampaignUnit { unit_key: key.into(), ..unit(id) }
}

fn command_land(m: &mut CampaignModel, c: i32, rank: i32) {
    m.world.character_details.entry(CharacterId(c)).or_default().attributes = vec![("command_land".into(), rank), ("command_sea".into(), -1)];
}

#[test]
fn commander_pick_follows_the_originals_unit_order() {
    let mut m = test_model();
    with_pick_units(&mut m);
    let f = ForceId(1001);
    m.world.forces.get_mut(&f).unwrap().units =
        vec![keyed(2, "test_unit"), keyed(3, "t_inf_flag"), keyed(4, "t_inf_rich"), keyed(5, "t_art"), keyed(6, "t_cav")];
    // Category first: cavalry 0, artillery 1, infantry 2.
    assert_eq!(m.commander_unit(f), Some(4));
    m.world.forces.get_mut(&f).unwrap().units.pop();
    assert_eq!(m.commander_unit(f), Some(3));
    m.world.forces.get_mut(&f).unwrap().units.pop();
    // Infantry: units #21 clear first, then the higher units #7.
    assert_eq!(m.commander_unit(f), Some(2));
    // A unit carrying a General beats every category; two Generals: the higher command_land.
    for (id, rank, ui) in [(103, 2, 0), (104, 5, 1)] {
        let mut g = character(id, B, CharacterKind::General, 30);
        g.position = pos(5, 5);
        m.world.characters.insert(g.id, g);
        command_land(&mut m, id, rank);
        m.world.forces.get_mut(&f).unwrap().units[ui].character = Some(CharacterId(id));
        assert_eq!(m.commander_unit(f), Some(ui));
    }
    // A colonel on a unit does not count as a General.
    m.world.characters.get_mut(&CharacterId(104)).unwrap().kind = CharacterKind::Colonel;
    assert_eq!(m.commander_unit(f), Some(0));
}

#[test]
fn a_general_riding_with_the_force_takes_command() {
    let mut m = test_model();
    let mut g = character(103, B, CharacterKind::General, 30);
    g.position = pos(5, 5);
    m.world.characters.insert(g.id, g);
    m.world.forces.get_mut(&ForceId(1001)).unwrap().units[1].character = Some(CharacterId(103));
    let pos101 = m.world.characters[&CharacterId(101)].position;
    let before = m.world.characters.len();
    m.character_dies(CharacterId(101));
    assert_eq!(m.world.forces[&ForceId(1001)].commander, Some(CharacterId(103)));
    // No new character; the new commander stands where the old one stood.
    assert_eq!(m.world.characters.len(), before - 1);
    assert_eq!(m.world.characters[&CharacterId(103)].position, pos101);
}

#[test]
fn a_fleet_gets_a_captain_on_its_first_ship_in_order() {
    let mut m = test_model();
    with_pick_units(&mut m);
    let mut adm = character(105, A, CharacterKind::Admiral, 40);
    adm.position = pos(3, 3);
    let cap = character(106, A, CharacterKind::Captain, 40);
    m.world.characters.insert(adm.id, adm);
    m.world.characters.insert(cap.id, cap);
    let mut frig = keyed(20, "t_frig");
    frig.character = Some(CharacterId(106));
    m.world.forces.insert(
        ForceId(1002),
        MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(105)), units: vec![frig, keyed(21, "t_line")], is_navy: true },
    );
    m.character_dies(CharacterId(105));
    // The ship of the line comes before the frigate; it has no captain, so a new one joins it
    // (the frigate's captain is no admiral and does not count).
    let f = &m.world.forces[&ForceId(1002)];
    let c = f.commander.expect("commander");
    assert_ne!(c, CharacterId(106));
    assert_eq!(f.units[1].character, Some(c));
    assert_eq!((m.world.characters[&c].kind, m.world.characters[&c].position), (CharacterKind::Captain, pos(3, 3)));
}

#[test]
fn a_returning_commander_escapes_and_his_force_is_handed_on() {
    let mut m = test_model();
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.push(unit(9));
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].character = Some(CharacterId(100));
    m.world.character_details.entry(CharacterId(100)).or_default().returns_after_death = true;
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.remove(0);
    m.character_falls(CharacterId(100));
    assert!(m.world.characters.contains_key(&CharacterId(100)));
    let f = &m.world.forces[&ForceId(1000)];
    let c = f.commander.expect("commander");
    assert_ne!(c, CharacterId(100));
    assert_eq!(m.world.characters[&c].kind, CharacterKind::Colonel);
}

#[test]
fn a_general_dying_of_old_age_loses_his_own_unit_first() {
    let mut m = test_model();
    // General 100 leads force 1000: his bodyguard (unit 1) and one more unit.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].character = Some(CharacterId(100));
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.push(unit(9));
    let year = m.calendar.date.year;
    m.world.character_details.entry(CharacterId(100)).or_default().birth = Some(Date { year: year - 120, season: 0, month: 0, half: HALF_EARLY });
    m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
    let pass = super::characters::yearly_character_pass(&mut m);
    assert!(pass.died.contains(&CharacterId(100)));
    let f = &m.world.forces[&ForceId(1000)];
    assert_eq!(f.units.iter().map(|u| u.id).collect::<Vec<_>>(), vec![UnitId(9)]);
    let c = f.commander.expect("commander");
    assert_eq!(f.units[0].character, Some(c));
    // A +0x52C character is not even checked.
    let mut m2 = test_model();
    m2.world.character_details.entry(CharacterId(100)).or_default().birth = Some(Date { year: year - 120, season: 0, month: 0, half: HALF_EARLY });
    m2.world.character_details.get_mut(&CharacterId(100)).unwrap().returns_after_death = true;
    m2.calendar.turn_in_year = m2.calendar.turns_per_year - 1;
    assert!(super::characters::yearly_character_pass(&mut m2).died.is_empty());
}

fn agent_details(m: &mut CampaignModel, c: i32, abilities: &[(&str, i32, &str)], attributes: &[(&str, i32)]) {
    let d = m.world.character_details.entry(CharacterId(c)).or_default();
    d.abilities = abilities.iter().map(|(k, l, a)| (k.to_string(), *l, a.to_string())).collect();
    d.attributes = attributes.iter().map(|(k, v)| (k.to_string(), *v)).collect();
}

/// A rake of A (103) and gentlemen of A (104) and B (105), all at (0, 0), with the abilities and
/// attributes the saves give them (rake: can_assassinate 1 + subterfuge 2; gentleman: can_duel 1).
fn agents_model() -> CampaignModel {
    let mut m = test_model();
    for (id, f, kind) in [(103, A, CharacterKind::Rake), (104, A, CharacterKind::Gentleman), (105, B, CharacterKind::Gentleman)] {
        m.world.characters.insert(CharacterId(id), character(id, f, kind, 20));
    }
    agent_details(&mut m, 103, &[("can_assassinate", 1, "subterfuge"), ("can_receive_duel", 1, "duelling_pistols")], &[("subterfuge", 2), ("duelling_pistols", 0), ("duelling_swords", 0)]);
    for g in [104, 105] {
        agent_details(&mut m, g, &[("can_duel", 1, "duelling_pistols"), ("can_receive_duel", 1, "duelling_pistols")], &[("duelling_pistols", 3), ("duelling_swords", 1), ("research", 2)]);
    }
    agent_details(&mut m, 101, &[], &[("command_land", 0)]);
    m
}

#[test]
fn agent_rolls_classify_as_the_original() {
    use super::agents::{classify, Outcome};
    // Chance 50, skill 2: critical success below 25 + 4 × 2 = 33, success from 33; failure below
    // trunc(100 − 50 × 0.15) = 92, critical from 92.
    assert_eq!(classify(50, 32, 2), Outcome::CriticalSuccess);
    assert_eq!(classify(50, 33, 2), Outcome::Success);
    assert_eq!(classify(50, 50, 2), Outcome::Success);
    assert_eq!(classify(50, 51, 2), Outcome::Failure);
    assert_eq!(classify(50, 92, 2), Outcome::CriticalFailure);
}

#[test]
fn duel_and_assassination_chances_follow_the_exe() {
    use super::agents::{assassination_chance, duel_chance, Weapon};
    let mut m = agents_model();
    // Duel: 3 / (3 + 3) with pistols; a rake cannot challenge.
    assert_eq!(duel_chance(&m, CharacterId(104), CharacterId(105), Weapon::Pistols), Some(50));
    m.world.character_details.get_mut(&CharacterId(105)).unwrap().attributes[0].1 = 1;
    assert_eq!(duel_chance(&m, CharacterId(104), CharacterId(105), Weapon::Pistols), Some(75));
    assert_eq!(duel_chance(&m, CharacterId(103), CharacterId(105), Weapon::Pistols), None);
    // The rake (skill 1 + 2 = 3) against B's gentleman (rank 2 in research): 76.92308 / 5 × 3.5.
    assert_eq!(assassination_chance(&m, CharacterId(103), CharacterId(105)), Some(53));
    // Against a colonel: impossible; a gentleman cannot assassinate.
    assert_eq!(assassination_chance(&m, CharacterId(103), CharacterId(101)), None);
    assert_eq!(assassination_chance(&m, CharacterId(104), CharacterId(105)), None);
    // Against a General (rank 2) leading two units: 3 / (2 + 1 + 0 + 3 + 0.2) × 100.
    m.world.characters.get_mut(&CharacterId(101)).unwrap().kind = CharacterKind::General;
    agent_details(&mut m, 101, &[], &[("command_land", 2)]);
    assert_eq!(assassination_chance(&m, CharacterId(103), CharacterId(101)), Some(48));
    // A protector with subterfuge in the target's force adds his rank: a rake of B attached to a unit.
    m.world.characters.insert(CharacterId(106), character(106, B, CharacterKind::Rake, 20));
    agent_details(&mut m, 106, &[], &[("subterfuge", 4)]);
    m.world.forces.get_mut(&ForceId(1001)).unwrap().units[1].character = Some(CharacterId(106));
    assert_eq!(super::agents::protector(&m, CharacterId(101)), Some(CharacterId(106)));
    // 3 / (2 + 1 + 4 + 3 + 0.2) × 100 = 29.4.
    assert_eq!(assassination_chance(&m, CharacterId(103), CharacterId(101)), Some(29));
}

#[test]
fn assassination_and_duel_commands_resolve_once_per_turn() {
    use super::agents::Outcome;
    let mut m = agents_model();
    let ev = m.apply(CampaignCommand::Assassinate { agent: CharacterId(103), target: CharacterId(105) }).unwrap();
    let outcome = ev
        .iter()
        .find_map(|e| match e {
            CampaignEvent::AgentActionResolved { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .expect("resolved");
    let target_alive = m.world.characters.contains_key(&CharacterId(105));
    let agent_alive = m.world.characters.contains_key(&CharacterId(103));
    match outcome {
        Outcome::CriticalSuccess | Outcome::Success => assert!(!target_alive && agent_alive),
        Outcome::Failure => assert!(target_alive && agent_alive),
        Outcome::CriticalFailure => assert!(target_alive && !agent_alive),
    }
    if agent_alive && target_alive {
        // Once per turn.
        assert!(m.apply(CampaignCommand::Assassinate { agent: CharacterId(103), target: CharacterId(105) }).is_err());
    }
    // Same faction: refused; a duel is fought once (the loser dies or flees: see
    // `a_duel_counts_and_the_loser_dies_or_flees`).
    let mut m = agents_model();
    assert_eq!(m.apply(CampaignCommand::Duel { challenger: CharacterId(104), target: CharacterId(103) }), Err(CommandError::WrongFaction));
    m.apply(CampaignCommand::Duel { challenger: CharacterId(104), target: CharacterId(105) }).unwrap();
    let won: u32 = [104, 105].iter().filter_map(|c| m.world.character_details.get(&CharacterId(*c))).map(|d| d.duels_won).sum();
    assert_eq!(won, 1);
    assert!(m.apply(CampaignCommand::Duel { challenger: CharacterId(104), target: CharacterId(105) }).is_err());

}
#[test]
fn only_the_first_recruitment_points_items_train_and_a_queue_holds_ten() {
    let mut m = test_model();
    m.world.factions.get_mut(&A).unwrap().treasury = 100_000;
    // Region 10 has 2 recruitment points: queue 10 items, the 11th is refused.
    for _ in 0..10 {
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    }
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }),
        Err(CommandError::NoRecruitmentCapacity)
    );
    m.turn.humans = vec![A];
    m.start_campaign();
    let turns: Vec<u32> = m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.turns_remaining).collect();
    assert_eq!(turns, vec![1, 1, 2, 2, 2, 2, 2, 2, 2, 2]);
}

#[test]
fn tax_level_changes_reach_the_governorship() {
    use super::details::{GovernmentPost, Governorship, GovernorshipTaxes};
    let mut m = test_model();
    let taxes = GovernorshipTaxes { lower: 2, upper: 2, lower_rate: 15, upper_rate: 15 };
    let g = Governorship { taxes, theatre_id: 1, regions: vec![RegionId(10)], faction: A, flags: (false, false) };
    m.world.faction_details.entry(A).or_default().posts =
        vec![GovernmentPost { id: 1, key: "governor_europe".into(), holder: None, governorship: Some(g) }];
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Lower, level: "tax_high".into() }).unwrap();
    let t = m.world.faction_details[&A].posts[0].governorship.as_ref().unwrap().taxes;
    assert_eq!(t, GovernorshipTaxes { lower: 3, upper: 2, lower_rate: 20, upper_rate: 15 });
    assert_eq!(m.world.factions[&A].tax_lower, "tax_high");
    // A modded rate past the save's u8 field reaches the model whole (it was clamped to 255).
    Arc::make_mut(&mut m.rules).tax_levels.insert("tax_extortionate".into(), 300);
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Upper, level: "tax_extortionate".into() }).unwrap();
    assert_eq!(m.world.faction_details[&A].posts[0].governorship.as_ref().unwrap().taxes.upper_rate, 300);
}

fn with_relationships(m: &mut CampaignModel, totals: &[((FactionId, FactionId), i32)]) {
    for a in [A, B, C] {
        for b in [A, B, C] {
            if a != b {
                let mut r = details::Relationship { attitudes: vec![details::AttitudeFactor::default(); 24], ..Default::default() };
                if let Some((_, v)) = totals.iter().find(|(k, _)| *k == (a, b)) {
                    r.attitudes[17].value = *v;
                }
                m.world.relationships.insert((a, b), r);
            }
        }
    }
}

#[test]
fn misdeeds_set_the_attitude_factors_from_the_config() {
    use super::agents::Misdeed;
    let mut m = agents_model();
    with_relationships(&mut m, &[]);
    // A caught at assassination against B: B -> A factor 14 = -50 drifting +2 to 0; C -> A -5, +1.
    m.detected_misdeed(A, B, Misdeed::Assassination);
    let f = &m.world.relationships[&(B, A)].attitudes[14];
    assert_eq!((f.value, f.drift, f.limit, f.limited), (-50, 2, 0, true));
    let f = &m.world.relationships[&(C, A)].attitudes[14];
    assert_eq!((f.value, f.drift), (-5, 1));
    assert_eq!(m.world.relationships[&(A, B)].attitudes[14].value, 0);
    // An unseen sabotage of B by A: B blames the faction it likes least other than A (C), and every
    // faction but A and C gives C the third-party values (B's own victim value overwritten).
    let mut m = agents_model();
    with_relationships(&mut m, &[((B, A), -10), ((B, C), -100)]);
    m.blamed_misdeed(A, B, Misdeed::Sabotage);
    assert_eq!(m.world.relationships[&(B, C)].attitudes[18].value, -2);
    assert_eq!(m.world.relationships[&(A, C)].attitudes[18].value, 0);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[18].value, 0);
}

#[test]
fn a_duel_counts_and_the_loser_dies_or_flees() {
    use super::agents::{AgentAction, Outcome};
    // Every seed: the loser of a critical outcome dies; the loser of an ordinary one flees to the
    // settlement of the region he stands in (both gentlemen stand near region 10's settlement in
    // the test model) and lives, marked as having fled (`CHARACTER` #27); `DuelFought` fires for both.
    let mut died = 0;
    let mut fled = 0;
    for seed in 1..=12u32 {
        let mut m = agents_model();
        m.rng = crate::rng::CaRng::new(seed);
        let ev = m.apply(CampaignCommand::Duel { challenger: CharacterId(104), target: CharacterId(105) }).unwrap();
        let outcome = ev
            .iter()
            .find_map(|e| match e {
                CampaignEvent::AgentActionResolved { action: AgentAction::Duel(_), outcome, .. } => Some(*outcome),
                _ => None,
            })
            .expect("resolved");
        assert_eq!(ev.iter().filter(|e| matches!(e, CampaignEvent::DuelFought { .. })).count(), 2);
        let (winner, loser) = if outcome.succeeded() { (104, 105) } else { (105, 104) };
        assert_eq!(m.world.character_details[&CharacterId(winner)].duels_won, 1);
        match outcome {
            Outcome::CriticalSuccess | Outcome::CriticalFailure => {
                assert!(!m.world.characters.contains_key(&CharacterId(loser)));
                died += 1;
            }
            Outcome::Success | Outcome::Failure => {
                let d = &m.world.character_details[&CharacterId(loser)];
                assert!(d.fled && !d.wounded);
                assert_eq!(d.duels_lost, 1);
                assert!(!m.stealthy(CharacterId(loser)));
                fled += 1;
            }
        }
    }
    assert!(died > 0 && fled > 0, "died {died}, fled {fled}");
}

#[test]
fn army_sabotage_stops_the_force_next_turn() {
    use super::agents::{army_sabotage_chance, guerilla_casualties};
    let mut m = agents_model();
    // The rake can sabotage armies; force 1001 (B, commander 101, 2 units).
    agent_details(&mut m, 103, &[("can_sabotage_army", 1, "subterfuge")], &[("subterfuge", 2)]);
    agent_details(&mut m, 101, &[], &[("command_land", 1)]);
    // 2 / ((2 + 1) × 0.33 + 2) × 100 = 66.9.
    assert_eq!(army_sabotage_chance(&m, CharacterId(103), ForceId(1001)), Some(66));
    // The mark zeroes the commander's action points at his next turn start, once.
    m.world.sabotaged.insert(ForceId(1001));
    m.begin_end_turn();
    let mut seen_zero = false;
    while m.step().is_some() {
        if m.world.sabotaged.is_empty() && !seen_zero {
            assert_eq!(m.world.characters[&CharacterId(101)].movement_points, 0);
            seen_zero = true;
        }
    }
    assert!(seen_zero);
    // A guerilla of rank 3 kills up to 30 men, at most half a unit at a time.
    let before: u32 = m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).sum();
    guerilla_casualties(&mut m, ForceId(1001), 3);
    let after: u32 = m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).sum();
    assert!(before - after <= 30 && after > 0);
}

#[test]
fn building_sabotage_damages_the_building() {
    use super::agents::building_sabotage_chance;
    let mut m = agents_model();
    agent_details(&mut m, 103, &[("can_sabotage", 1, "subterfuge")], &[("subterfuge", 2)]);
    // A building of B's region 11.
    let (slot, level) = {
        let r = m.world.regions.get_mut(&RegionId(11)).unwrap();
        let i = r.slots.iter().position(|s| s.building.is_some()).unwrap_or(0);
        if r.slots[i].building.is_none() {
            r.slots[i].building = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
        }
        r.slots[i].position = Some(pos(0, 0));
        (i, m.rules.buildings[&r.slots[i].building.as_ref().unwrap().level_key].level)
    };
    // S = rank 2; k = 3 (chain number 0): 2 / (round((level + 1) × 3) + 2) × 100.
    let expect = ((2.0 / (((level + 1) as f32 * 3.0).round() + 2.0)) * 100.0) as i32;
    assert_eq!(building_sabotage_chance(&m, CharacterId(103), RegionId(11), slot), Some(expect.clamp(5, 95)));
    let ev = m.apply(CampaignCommand::SabotageBuilding { agent: CharacterId(103), region: RegionId(11), slot }).unwrap();
    let outcome = ev.iter().find_map(|e| match e {
        CampaignEvent::AgentActionResolved { outcome, .. } => Some(*outcome),
        _ => None,
    });
    let health = m.world.regions[&RegionId(11)].slots[slot].building.as_ref().unwrap().health;
    match outcome.expect("resolved") {
        super::agents::Outcome::CriticalSuccess => assert_eq!(health, 29), // 100 × (1 − 0.71)
        super::agents::Outcome::Success => assert_eq!(health, 53),         // 100 × (1 − 0.46)
        _ => assert_eq!(health, 100),
    }
}

#[test]
fn foreign_gentlemen_steal_or_are_thrown_out() {
    use super::agents::steal_chance;
    // (skill + 1) × 0.5 / √cost, 0.04..=0.9.
    assert!((steal_chance(3, 100) - 0.2).abs() < 1e-6);
    assert_eq!(steal_chance(0, 10_000), 0.04);
    assert_eq!(steal_chance(12, 1), 0.9);
    let mut m = agents_model();
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("t_steal".into(), super::rules::TechRules { cost: 4, ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies.push(("t_steal".into(), 2));
    m.world.regions.get_mut(&RegionId(11)).unwrap().slots[0].position = Some(pos(0, 0));
    // A's gentleman (research 2) at B's school: chance 1.5 / 2 = 0.75, capped 0.9.
    let res = m.steal_step(RegionId(11), 0, "t_steal");
    assert_eq!(res.len(), 1);
    let stolen = res[0].1;
    let state = m.world.faction_details[&A].technologies.iter().find(|t| t.0 == "t_steal").unwrap().1;
    assert_eq!(stolen, state == 0);
    if !stolen {
        assert_ne!(m.world.characters[&CharacterId(104)].position, pos(0, 0));
    }
}

#[test]
fn queued_units_the_region_can_no_longer_recruit_are_dropped_and_refunded() {
    let mut m = test_model();
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    let after = m.world.factions[&A].treasury;
    // The building that allows the unit is gone: at the region update the item is removed, refunded.
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building = None;
    m.turn.humans = vec![A];
    m.start_campaign();
    assert!(m.world.regions[&RegionId(10)].recruitment_queue.is_empty());
    assert_eq!(m.world.factions[&A].treasury, after + 400);
}

/// The test model plus a `test_depot` level that allows only `test_guard` (a copy of `test_recruit`) and has no
/// effects, standing in region 10's second slot.
fn model_with_a_depot() -> CampaignModel {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    let guard = rules.units["test_recruit"].clone();
    rules.units.insert("test_guard".into(), guard);
    rules.buildings.insert(
        "test_depot".into(),
        super::rules::BuildingRules { chain: "test_chain".into(), units_allowed: vec!["test_guard".into(), "test_recruit".into()], ..Default::default() },
    );
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[1].building = Some(BuildingRef { level_key: "test_depot".into(), health: 100 });
    m.world.factions.get_mut(&A).unwrap().treasury = 10_000;
    m
}

/// The region's recruitable list carries the building flags of `0x00B43CA0`: 8 for a damaged building, 0x10 for
/// a slot another faction holds, 0x80 for a unit whose technology is not researched (`0x008AAB60`). A unit
/// allowed by two buildings is unflagged when one of them is (`0x00B08E30`), else carries both flags; the queue
/// command refuses a flagged entry and charges nothing.
#[test]
fn the_recruitable_list_flags_damaged_and_occupied_buildings_and_missing_technologies() {
    use super::commands::{RecruitableUnit, ENTRY_DAMAGED, ENTRY_NO_TECHNOLOGY, ENTRY_OCCUPIED};
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    let entry = |k: &str, flags: u32| RecruitableUnit { unit_key: k.into(), flags };
    // Sorted by unit key (`0x00B78140`), not in building order.
    let all_clear = vec![entry("test_guard", 0), entry("test_recruit", 0), entry("test_unit", 0)];
    assert_eq!(m.recruitable_units(r10), all_clear);
    // The barracks is damaged: its own unit is flagged, the one the depot also allows is not.
    m.world.regions.get_mut(&r10).unwrap().slots[0].building.as_mut().unwrap().health = 99;
    assert_eq!(m.recruitable_units(r10), vec![entry("test_guard", 0), entry("test_recruit", 0), entry("test_unit", ENTRY_DAMAGED)]);
    // B holds the depot's slot as well: both sources flagged, so their flags add up.
    m.world.regions.get_mut(&r10).unwrap().slots[1].holder = Some(B);
    let flagged = vec![entry("test_guard", ENTRY_OCCUPIED), entry("test_recruit", ENTRY_DAMAGED | ENTRY_OCCUPIED), entry("test_unit", ENTRY_DAMAGED)];
    assert_eq!(m.recruitable_units(r10), flagged);
    // Holding one's own slot is no occupation.
    m.world.regions.get_mut(&r10).unwrap().slots[1].holder = Some(A);
    m.world.regions.get_mut(&r10).unwrap().slots[0].building.as_mut().unwrap().health = 100;
    assert_eq!(m.recruitable_units(r10), all_clear);
    // A technology the faction has not researched flags the unit, wherever it is allowed.
    let mut rules = (*m.rules).clone();
    rules.unit_techs.insert("test_recruit".into(), vec!["test_tech".into()]);
    m.rules = Arc::new(rules);
    assert_eq!(m.recruitable_units(r10)[1], entry("test_recruit", ENTRY_NO_TECHNOLOGY));
    let treasury = m.world.factions[&A].treasury;
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: r10, unit_key: "test_recruit".into(), target: None }),
        Err(CommandError::RecruitmentBlocked { unit_key: "test_recruit".into(), flags: ENTRY_NO_TECHNOLOGY })
    );
    assert_eq!((m.world.factions[&A].treasury, m.world.regions[&r10].recruitment_queue.len()), (treasury, 0));
    // The pricing flags come on top of the building flags.
    let unit = m.rules.units["test_recruit"].clone();
    m.world.factions.get_mut(&A).unwrap().treasury = 0;
    let flags = m.recruitable_entry_flags(&m.world.regions[&r10], &m.recruitable_units(r10)[1], &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags, ENTRY_NO_TECHNOLOGY | super::commands::ENTRY_TOO_DEAR);
}

/// A unit whose `units_to_gov_type_permissions` rows enable some government types is listed only under one of
/// them (`0x00EA9810`); unlike the flags, the unit then has no entry, so the command reports it unavailable.
#[test]
fn a_unit_enabled_for_other_governments_is_not_recruitable() {
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    let listed = |m: &CampaignModel| m.recruitable_units(r10).iter().any(|e| e.unit_key == "test_guard");
    let mut rules = (*m.rules).clone();
    rules.unit_governments.insert("test_guard".into(), vec!["gov_republic".into()]);
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().government_key = "gov_absolute_monarchy".into();
    assert!(!listed(&m));
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: r10, unit_key: "test_guard".into(), target: None }),
        Err(CommandError::UnitNotAvailable("test_guard".into()))
    );
    m.world.factions.get_mut(&A).unwrap().government_key = "gov_republic".into();
    assert!(listed(&m));
    // No enabled government type (only `destroyed` rows, say) is no restriction.
    let mut rules = (*m.rules).clone();
    rules.unit_governments.insert("test_guard".into(), Vec::new());
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().government_key = "gov_absolute_monarchy".into();
    assert!(listed(&m));
}

/// The queue step holds back an item whose recruitable entry is flagged (`0x00B5AD90`): it neither counts down
/// nor takes one of the `recruitment_points`, so the items behind it train in its place; it is not removed or
/// refunded, and counts down again once its building is repaired.
#[test]
fn a_flagged_item_is_held_back_without_taking_a_recruitment_point() {
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    // Two points (the barracks); the guard first, then two recruits.
    for (id, unit) in [(9001, "test_guard"), (9002, "test_recruit"), (9003, "test_recruit")] {
        let item = RecruitmentItem { id: RecruitmentItemId(id), unit_key: unit.into(), turns_remaining: 3, cost: 400, target: None };
        m.world.regions.get_mut(&r10).unwrap().recruitment_queue.push(item);
    }
    assert_eq!(m.recruitment_points(r10, false), 2);
    m.world.regions.get_mut(&r10).unwrap().slots[1].building.as_mut().unwrap().health = 50;
    let treasury = m.world.factions[&A].treasury;
    m.turn.humans = vec![A];
    m.start_campaign();
    let turns = |m: &CampaignModel| m.world.regions[&r10].recruitment_queue.iter().map(|i| (i.id.0, i.turns_remaining)).collect::<Vec<_>>();
    assert_eq!(turns(&m), vec![(9001, 3), (9002, 2), (9003, 2)]);
    assert_eq!(m.world.factions[&A].treasury, treasury, "a held-back item is not refunded");
    // Repaired, the guard takes its point again, and the last recruit waits.
    m.world.regions.get_mut(&r10).unwrap().slots[1].building.as_mut().unwrap().health = 100;
    m.end_turn();
    assert_eq!(turns(&m), vec![(9001, 2), (9002, 1), (9003, 2)]);
}

#[test]
fn hidden_characters_are_known_once_exposed() {
    use super::agents::knows_character;
    let mut m = agents_model();
    let rake = CharacterId(103);
    // A rake (subterfuge 2) is unknown to B until B spots him; his own faction always knows him.
    assert!(knows_character(&m, A, rake));
    assert!(!knows_character(&m, B, rake));
    m.expose(B, rake);
    assert!(knows_character(&m, B, rake));
    // The exe pushes a subterfuge character again without looking.
    m.expose(B, rake);
    assert_eq!(m.world.faction_details[&B].exposed, vec![rake, rake]);
    // Anyone else is known, unless flagged hidden (#22); then added once.
    let gent = CharacterId(104);
    assert!(knows_character(&m, B, gent));
    m.world.character_details.get_mut(&gent).unwrap().hidden = true;
    assert!(!knows_character(&m, B, gent));
    m.expose(B, gent);
    m.expose(B, gent);
    assert_eq!(m.world.faction_details[&B].exposed.iter().filter(|c| **c == gent).count(), 1);
    // A dead character leaves every list.
    m.character_dies(rake);
    assert!(!m.world.faction_details[&B].exposed.contains(&rake));
}

#[test]
fn spying_rolls_and_a_detected_spy_is_exposed() {
    use super::agents::{spy_chance, Outcome, SpyTarget};
    let mut m = agents_model();
    let rake = CharacterId(103);
    // No `can_spy`, no spying.
    assert_eq!(spy_chance(&m, rake, SpyTarget::Force(ForceId(1001))), None);
    agent_details(&mut m, 103, &[("can_spy", 1, "subterfuge")], &[("subterfuge", 2)]);
    // S = rank 2: on a force of 2 units 2 / (2 + 1) = 66 %, on a settlement 2 / 2.5 = 80 %.
    assert_eq!(spy_chance(&m, rake, SpyTarget::Force(ForceId(1001))), Some(66));
    assert_eq!(spy_chance(&m, rake, SpyTarget::Settlement(RegionId(11))), Some(80));
    // Not on his own side.
    assert_eq!(spy_chance(&m, rake, SpyTarget::Settlement(RegionId(10))), None);
    m.world.regions.get_mut(&RegionId(11)).unwrap().settlement.position = pos(0, 0);
    let ev = m.apply(CampaignCommand::Spy { agent: rake, target: SpyTarget::Settlement(RegionId(11)) }).unwrap();
    let outcome = ev.iter().find_map(|e| match e {
        CampaignEvent::AgentActionResolved { outcome, .. } => Some(*outcome),
        _ => None,
    });
    let exposed = m.world.faction_details.get(&B).is_some_and(|d| d.exposed.contains(&rake));
    match outcome.expect("resolved") {
        Outcome::CriticalSuccess | Outcome::Success => assert!(!exposed),
        Outcome::Failure => assert!(exposed && m.world.characters.contains_key(&rake)),
        Outcome::CriticalFailure => assert!(!m.world.characters.contains_key(&rake)),
    }
    assert!(m.apply(CampaignCommand::Spy { agent: rake, target: SpyTarget::Settlement(RegionId(11)) }).is_err());
}

#[test]
fn capture_damage_roll_matches_the_listing() {
    use super::capture::{damage_roll, LOOT_DAMAGE, OCCUPY_DAMAGE};
    // CaRng::new(0).next16() = 2531011 >> 16 = 38: frac = 0.49 × 38 / 65535 + lo. Value in f32 as the listing:
    // 99 × 0.01 = 0.98999995, × 1000 = 989.99994 → 989, × 4.
    assert_eq!(damage_roll(&mut CaRng::new(0), 100, 1000, LOOT_DAMAGE, (4, 15_000)), (1, 3956));
    assert_eq!(damage_roll(&mut CaRng::new(0), 100, 1000, OCCUPY_DAMAGE, (4, 15_000)), (50, 2000));
    // Above the cap the value is a quarter: min(395996, max(15000, 98999)).
    assert_eq!(damage_roll(&mut CaRng::new(0), 100, 100_000, LOOT_DAMAGE, (4, 15_000)), (1, 98_999));
    // spa: × 2, cap 10000.
    assert_eq!(damage_roll(&mut CaRng::new(0), 100, 1000, LOOT_DAMAGE, (2, 10_000)), (1, 1978));
}

/// Region 11 (B's) with a town building, war between A and B, A's army next to it.
fn capture_model(humans: Vec<FactionId>) -> CampaignModel {
    let mut m = test_model();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(-20, 0);
    let r = m.world.regions.get_mut(&RegionId(11)).unwrap();
    r.slots[0].key = "settlement:test_region_11:town:test_slot:0".into();
    r.town_wealth = 1000;
    if !humans.is_empty() {
        m.turn = TurnState::in_turn_of(A, humans);
    }
    m
}

#[test]
fn human_capture_waits_for_the_choice_then_loots() {
    use super::capture::{damage_roll, CaptureChoice, LOOT_DAMAGE, OCCUPY_DAMAGE};
    let mut m = capture_model(vec![A]);
    let mut rng = m.rng;
    let (loot_h, value) = damage_roll(&mut rng, 100, 300, LOOT_DAMAGE, (4, 15_000));
    let (occ_h, _) = damage_roll(&mut rng, 100, 300, OCCUPY_DAMAGE, (4, 15_000));
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CaptureChoicePending { region: RegionId(11), faction: A }));
    assert_eq!(m.rng, rng, "both passes draw, loot first");
    let p = m.pending_capture.clone().unwrap();
    // Money: rolls + clamp(0.15 × 1000, 150, 7000) + clamp(0.15 × 0.15 × 8000, 150, 6000), × 1 (no looting bonus).
    assert_eq!(p.loot.money, value as i32 + 150 + 180);
    assert_eq!(p.loot.buildings, vec![(0, loot_h)]);
    assert_eq!(p.loot.town_wealth, 200);
    assert_eq!(p.loot.public_order_reduction, 10);
    assert_eq!(p.occupy, super::capture::CaptureOutcome { buildings: vec![(0, occ_h)], town_wealth: 1000, public_order_after: p.occupy.public_order_after, ..Default::default() });
    assert!(p.loot.public_order_after.is_some());
    assert_eq!(p.liberate, None);
    let before = m.world.factions[&A].treasury;
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
    assert!(ev.contains(&CampaignEvent::CaptureResolved { region: RegionId(11), faction: A, choice: CaptureChoice::Loot, money: p.loot.money }));
    assert!(m.pending_capture.is_none());
    assert_eq!(m.world.factions[&A].treasury, before + p.loot.money);
    let r = &m.world.regions[&RegionId(11)];
    assert_eq!(r.slots[0].building.as_ref().unwrap().health, loot_h);
    assert_eq!(r.town_wealth, 200);
    assert!(r.class_bases.iter().all(|c| c.2 == -10), "{:?}", r.class_bases);
    // Repair: round((100 − h) × 300 × 0.01) with no cost modifier, over max(1, floor((100 − h) × 0.02)) turns.
    assert!(m.can_repair(RegionId(11), SlotRef::Slot(0)));
    let cost = ((100 - loot_h) as f64 * 3.0).round_ties_even() as i32;
    assert_eq!(m.repair_cost(RegionId(11), SlotRef::Slot(0)), cost);
    let t = m.world.factions[&A].treasury;
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(11), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, t - cost);
    let item = &m.world.regions[&RegionId(11)].construction[0];
    assert_eq!((item.slot, item.level_key.as_str(), item.turns_remaining), (SlotRef::Slot(0), "test_building_level", (((100 - loot_h) as f32 * 0.02).floor() as u32).max(1)));
    assert!(!m.can_repair(RegionId(11), SlotRef::Slot(0)), "already being repaired");
}

#[test]
fn ai_capture_occupies_and_liberation_hands_the_region_over() {
    use super::capture::{damage_roll, CaptureChoice, LOOT_DAMAGE, OCCUPY_DAMAGE};
    // AI: occupied at once (PROVISIONAL default), the civil building damaged by the occupy roll.
    let mut m = capture_model(Vec::new());
    let mut rng = m.rng;
    damage_roll(&mut rng, 100, 300, LOOT_DAMAGE, (4, 15_000));
    let (occ_h, _) = damage_roll(&mut rng, 100, 300, OCCUPY_DAMAGE, (4, 15_000));
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CaptureResolved { region: RegionId(11), faction: A, choice: CaptureChoice::Occupy, money: 0 }));
    assert!(m.pending_capture.is_none());
    assert_eq!(m.world.regions[&RegionId(11)].slots[0].building.as_ref().unwrap().health, occ_h);
    assert_eq!(m.world.regions[&RegionId(11)].town_wealth, 1000);
    // A military chain is spared by the occupy pass.
    let mut m = capture_model(Vec::new());
    Arc::make_mut(&mut m.rules).chain_kinds.insert("test_chain".into(), 1);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].slots[0].building.as_ref().unwrap().health, 100);

    // Liberation: the region's rebel faction is C, which holds no region.
    let mut m = capture_model(vec![A]);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(m.pending_capture.as_ref().unwrap().liberate, Some(C));
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, C);
    // The liberation (0x00B58A10) moves no army: the capturer's army stays inside, a foreign garrison.
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, Some(RegionId(11)));
    assert_eq!(m.world.regions[&RegionId(11)].garrison, Some(ForceId(1000)));
    // Not offered when the relationship forbids returning regions, nor for the capturer itself.
    let mut m = capture_model(vec![A]);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
    m.world.relationships.insert((A, C), Relationship { allows_region_return: false, ..Default::default() });
    assert_eq!(m.liberation_target(RegionId(11), A), None);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_a".into());
    assert_eq!(m.liberation_target(RegionId(11), A), None);
    // An open capture is settled as an occupation when the turn ends.
    let mut m = capture_model(vec![A]);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    let ev = m.apply(CampaignCommand::EndTurn).unwrap();
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CaptureResolved { choice: CaptureChoice::Occupy, .. })));
}

#[test]
fn attitude_factor_writes_and_drift_follow_the_exe() {
    use super::details::AttitudeFactor;
    use super::treaties::{attitude_category, event};
    let mut f = AttitudeFactor::default();
    f.set(event("war"));
    assert_eq!((f.value, f.drift, f.limit, f.limited), (-140, -2, -200, true));
    for _ in 0..40 {
        f.step();
    }
    assert_eq!(f.value, -200, "drifts down to the limit");
    // Peace adds +120 to the war factor, which then recovers by 2 a turn up to 0.
    let p = event("peace");
    f.add(p.value, p.drift, p.limit);
    assert_eq!((f.value, f.drift, f.limit), (-80, 2, 0));
    for _ in 0..50 {
        f.step();
    }
    assert_eq!(f.value, 0);
    // The shipped thresholds: boundaries at -65, -22, 21, 64.
    let t = BTreeMap::new();
    let cats: Vec<u8> = [-65, -64, -22, -21, 21, 22, 64, 65].iter().map(|&x| attitude_category(&t, x)).collect();
    assert_eq!(cats, vec![0, 1, 1, 2, 2, 3, 3, 4]);
    // The one name list (the diplomat lines and the diplomacy UI; regression: three copies, the UI's with its own
    // boundary rule).
    let names: Vec<&str> = cats.iter().map(|&c| super::treaties::attitude_name(c)).collect();
    assert_eq!(names, ["hostile", "unfriendly", "unfriendly", "neutral", "neutral", "friendly", "friendly", "very_friendly"]);
}

#[test]
fn war_peace_and_third_parties_change_the_relationships() {
    let mut m = test_model();
    // C likes B very much (category 4): A's war on B costs A with C.
    m.relationship_mut(C, B).attitudes[super::treaties::slot("alliance")].value = 80;
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    for (x, y) in [(A, B), (B, A)] {
        let w = m.world.relationships[&(x, y)].attitudes[super::treaties::slot("war")];
        assert_eq!((w.value, w.drift, w.limit), (-140, -2, -200));
    }
    let f = m.world.relationships[&(C, A)].attitudes[super::treaties::slot("declared_war_against_friends")];
    assert_eq!((f.value, f.drift, f.limit), (-15, 1, 0), "war_against_friends added; it recovers to 0");
    m.apply(CampaignCommand::MakePeace { a: A, b: B }).unwrap();
    let r = &m.world.relationships[&(A, B)];
    assert_eq!(r.attitudes[super::treaties::slot("war")].value, -20);
    assert_eq!(r.friendship_turns, 10);
    // Declaring war again during the friendship turns is backstabbing: the count grows by 3 (10 turns).
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(m.world.treaty_breaks[&A], 3);
}

#[test]
fn treaties_and_the_per_turn_update() {
    use super::treaties::{slot, DiplomaticAction as D};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    for k in ["test_faction_a", "test_faction_b", "test_faction_c"] {
        rules.faction_subcultures.insert(k.into(), "sc_test".into());
    }
    m.rules = Arc::new(rules);
    let act = |m: &mut CampaignModel, a, b, action| m.apply(CampaignCommand::Diplomacy { a, b, action }).unwrap();
    // Alliance: 30 rising to 80, commitment 20.
    act(&mut m, A, B, D::Alliance);
    assert_eq!(m.world.stance(B, A), Stance::Allied);
    let r = &m.world.relationships[&(B, A)];
    assert_eq!((r.attitudes[slot("alliance")].value, r.alliance_commitment_turns), (30, 20));
    // Breaking it while committed: B resents it, C (same subculture) too.
    act(&mut m, A, B, D::BreakAlliance);
    assert_eq!(m.world.stance(A, B), Stance::Neutral);
    let ab = m.world.relationships[&(B, A)].attitudes[slot("alliance_broken")];
    assert_eq!((ab.value, ab.drift, ab.limit), (-40, 2, 0));
    let cb = m.world.relationships[&(C, A)].attitudes[slot("cultural_alliance_broken")];
    assert_eq!((cb.value, cb.drift, cb.limit), (-20, 1, 0));
    assert_eq!(m.world.alliances_broken[&A], 1);
    // Trade, then breaking it, then an embargo.
    act(&mut m, A, B, D::TradeAgreement);
    assert!(m.world.relationships[&(B, A)].trade_agreement);
    act(&mut m, A, B, D::BreakTrade);
    let tb = m.world.relationships[&(B, A)].attitudes[slot("trade_broken")];
    assert_eq!((tb.value, m.world.relationships[&(B, A)].trade_agreement), (-20, false));
    act(&mut m, A, B, D::Embargo);
    assert_eq!(m.world.relationships[&(A, B)].trade_embargo_turns, 10);
    // Military access for 10 turns, cancelled after 2: grievance 60 − 6 × 2.
    act(&mut m, A, B, D::GrantMilitaryAccess(10));
    m.diplomacy_round_end(A);
    m.diplomacy_round_end(A);
    let r = &m.world.relationships[&(A, B)];
    assert_eq!((r.military_access_turns, r.military_access_elapsed, r.military_access_streak), (8, 2, 2));
    act(&mut m, A, B, D::CancelMilitaryAccess);
    assert_eq!(m.world.relationships[&(A, B)].access_cancel_grievance, 48);
    assert_eq!(m.world.access_cancel_marks[&A], 16);
    // The per-turn update: the grievance falls by 2, the embargo by 1, the alliance_broken factor recovers by 2.
    let before = m.world.relationships[&(B, A)].attitudes[slot("alliance_broken")].value;
    m.diplomacy_round_end(A);
    m.diplomacy_round_end(B);
    let r = &m.world.relationships[&(A, B)];
    assert_eq!((r.access_cancel_grievance, r.trade_embargo_turns), (46, 7), "three updates since the embargo");
    assert_eq!(m.world.relationships[&(B, A)].attitudes[slot("alliance_broken")].value, before + 2);
    // A state gift charges the giver and pleases the receiver, who gets no money (0x00C4B440); a payment adds an item.
    let (ta, tb) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
    act(&mut m, A, B, D::StateGift(500));
    assert_eq!((m.world.factions[&A].treasury, m.world.factions[&B].treasury), (ta - 500, tb));
    assert!(m.world.relationships[&(B, A)].attitudes[slot("state_gift")].value > 0);
    // A deal's one-off payment moves the money (0x00C18A70), a negative one the other way, with no money test.
    act(&mut m, A, B, D::OneOffPayment(300));
    assert_eq!((m.world.factions[&A].treasury, m.world.factions[&B].treasury), (ta - 800, tb + 300));
    act(&mut m, A, B, D::OneOffPayment(-100));
    assert_eq!((m.world.factions[&A].treasury, m.world.factions[&B].treasury), (ta - 700, tb + 200));
    m.world.factions.get_mut(&B).unwrap().treasury = 0;
    act(&mut m, B, A, D::OneOffPayment(50));
    assert_eq!((m.world.factions[&A].treasury, m.world.factions[&B].treasury), (ta - 650, -50));
    act(&mut m, A, B, D::RegularPayment(100, 3));
    assert_eq!(m.world.relationships[&(A, B)].payments.len(), 1);
    for _ in 0..3 {
        m.diplomacy_round_end(A);
    }
    assert!(m.world.relationships[&(A, B)].payments.is_empty());
    // Protectorate: B becomes A's protectorate.
    act(&mut m, B, A, D::BecomeProtectorate);
    assert_eq!((m.world.stance(B, A), m.world.stance(A, B)), (Stance::Protectorate, Stance::Patron));
}

#[test]
fn religion_conversion_follows_the_exe_formula() {
    let mut m = test_model_with([("conversion_rel_a".to_string(), vec![EffectKey { kind: BonusKind::Religion, bonus: "conversion".into(), qualifier: "rel_a".into() }])]);
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("conversion_rel_a", 3.0)]));
    rules.conversion_mods.insert(("rel_b".into(), "rel_a".into()), -0.2);
    m.rules = Arc::new(rules);
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    r.population = 100_000;
    r.religions = vec![("rel_a".into(), 0.5), ("rel_b".into(), 0.5)];
    // s = 3, d = 3: x = 1.9 + 0.05 × 9 − 0.2 = 2.15; amount = (200 + 0.004 × 50000) × 2.4 × 2.15 = 2064.
    let flows = m.conversion_flows(RegionId(10));
    assert_eq!(flows.len(), 1);
    assert!((flows[0].amount - 2064.0).abs() < 0.01, "{flows:?}");
    m.convert_region(RegionId(10));
    let rel = &m.world.regions[&RegionId(10)].religions;
    assert!((rel[0].1 - 0.52064).abs() < 1e-5 && (rel[1].1 - 0.47936).abs() < 1e-5, "{rel:?}");
}

#[test]
fn capture_rolls_the_fortification_after_the_town_buildings() {
    use super::capture::{damage_roll, CaptureChoice, LOOT_DAMAGE, OCCUPY_DAMAGE};
    let mut m = capture_model(vec![A]);
    m.world.regions.get_mut(&RegionId(11)).unwrap().fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    let mut rng = m.rng;
    damage_roll(&mut rng, 100, 300, LOOT_DAMAGE, (4, 15_000));
    let (fort_loot, _) = damage_roll(&mut rng, 100, 300, LOOT_DAMAGE, (4, 15_000));
    damage_roll(&mut rng, 100, 300, OCCUPY_DAMAGE, (4, 15_000));
    let (fort_occ, _) = damage_roll(&mut rng, 100, 300, OCCUPY_DAMAGE, (4, 15_000));
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(m.rng, rng);
    let p = m.pending_capture.clone().unwrap();
    assert_eq!((p.loot.fortification, p.occupy.fortification), (Some(fort_loot), Some(fort_occ)));
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].fortification.as_ref().unwrap().health, fort_occ);
}

#[test]
fn allies_are_called_and_treaty_money_moves() {
    use super::treaties::{slot, DiplomaticAction as D};
    let mut m = test_model();
    // C needs land to be in the game.
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    let act = |m: &mut CampaignModel, a, b, action| m.apply(CampaignCommand::Diplomacy { a, b, action }).unwrap();
    act(&mut m, B, C, D::Alliance);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    // C (AI, allied to B) joins B's war against A, dragged in by its ally.
    assert_eq!(m.world.stance(C, A), Stance::War);
    let r = &m.world.relationships[&(C, A)];
    assert_eq!((r.war_ally, r.attitudes[slot("war")].value), (Some(B), -70));
    assert_eq!(m.world.relationships[&(C, B)].allied_in_war_against.len(), 1);
    assert_eq!(m.world.relationships[&(C, B)].military_access_turns, -1);
    // An ally of both sides refuses the attacked side (breaking that alliance), then is called by the attacker.
    let mut m = test_model();
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    act(&mut m, B, C, D::Alliance);
    act(&mut m, A, C, D::Alliance);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!((m.world.stance(C, B), m.world.stance(C, A)), (Stance::War, Stance::Allied));
    assert!(m.world.relationships[&(B, C)].attitudes[slot("alliance_broken")].value < 0);
    // Money: a payment moves each turn; a protectorate pays a fifth of its revenue.
    let mut m = test_model();
    act(&mut m, A, B, D::RegularPayment(100, 2));
    let (ta, tb) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
    m.diplomacy_money(A);
    assert_eq!((m.world.factions[&A].treasury, m.world.factions[&B].treasury), (ta - 100, tb + 100));
    act(&mut m, B, A, D::BecomeProtectorate);
    let revenue = economy::faction_income(&m, B).revenue();
    let (ta, tb) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
    m.diplomacy_money(B);
    assert_eq!((m.world.factions[&A].treasury, m.world.factions[&B].treasury), (ta + revenue / 5, tb - revenue / 5));
    assert_eq!(m.world.relationships[&(A, B)].protectorate_income, revenue / 5);
}

#[test]
fn fortifications_are_built_and_repaired() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 1000, turns: 2, upgrades_to: vec!["test_keep".into()], ..Default::default() });
    rules.buildings.insert("test_keep".into(), BuildingRules { chain: "test_fort".into(), level: 1, cost: 3000, turns: 3, ..Default::default() });
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    let t = m.world.factions[&A].treasury;
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 4000);
    let _ = t;
    m.end_turn();
    m.end_turn();
    assert_eq!(m.world.regions[&RegionId(10)].fortification.as_ref().map(|b| b.level_key.as_str()), Some("test_walls"));
    assert!(m.can_build(RegionId(10), SlotRef::Walls, "test_keep").is_ok(), "intact walls can be upgraded");
    // Damaged to 40: repair costs round(1000 × 0.6) and takes floor(0.6 × 2) = 1 turn.
    m.world.regions.get_mut(&RegionId(10)).unwrap().fortification.as_mut().unwrap().health = 40;
    // ... and the command refuses to upgrade it: one rule for the panel and the command.
    assert!(m.can_build(RegionId(10), SlotRef::Walls, "test_keep").is_err(), "no upgrade of damaged walls");
    // An AI owner repairs its damaged walls at its turn start (`ai_repairs`), so they can be
    // upgraded again afterwards.
    {
        let mut ai = m.clone();
        ai.turn.humans = Vec::new();
        ai.world.factions.get_mut(&A).unwrap().treasury = 5000;
        ai.ai_repairs(A);
        assert_eq!(ai.world.regions[&RegionId(10)].construction.last().map(|c| c.slot), Some(SlotRef::Walls), "the AI repairs its walls");
    }
    assert!(m.can_repair(RegionId(10), SlotRef::Walls));
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Walls), 600);
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.turn.humans = vec![A];
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Walls }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 4400);
    assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| (c.slot, c.turns_remaining)), Some((SlotRef::Walls, 1)));
    // The repair's length is the model's one rule, also while it runs (the health stays until it ends).
    assert_eq!(m.repair_turns(RegionId(10), SlotRef::Walls), 1);
    m.world.regions.get_mut(&RegionId(10)).unwrap().fortification.as_mut().unwrap().health = 0;
    assert_eq!(m.repair_turns(RegionId(10), SlotRef::Walls), 2);
}

/// A damaged road is never repaired (PROVISIONAL: the exe's road repair is not traced, nothing
/// damages a road, and the original's infrastructure panel hides the repair button).
#[test]
fn a_road_is_not_repaired() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.world.regions.get_mut(&RegionId(10)).unwrap().road = Some(BuildingRef { level_key: "test_building_level".into(), health: 40 });
    assert!(!m.can_repair(RegionId(10), SlotRef::Road));
    assert!(matches!(m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Road }), Err(CommandError::CannotBuild(_))));
    assert!(m.world.regions[&RegionId(10)].construction.is_empty());
    assert!(!m.can_demolish(RegionId(10), SlotRef::Road), "nor demolished");
}

/// A construction item for a slot the region does not have (review: it never progressed, kept the
/// region `constructing` for good and the AI skipped the region) is dropped at the region's turn,
/// while the items of real slots go on. The drop is reported once (a diagnostic event the app logs, never
/// sent to the scripts; review: it was silent).
#[test]
fn a_construction_item_for_a_missing_slot_is_dropped() {
    let mut m = test_model();
    let item = |slot| ConstructionItem { slot, level_key: "test_building_level".into(), turns_remaining: 3, cost: 0 };
    m.world.regions.get_mut(&RegionId(10)).unwrap().construction = vec![item(SlotRef::Slot(99)), item(SlotRef::Slot(1))];
    let events = m.end_turn();
    let left: Vec<(SlotRef, u32)> = m.world.regions[&RegionId(10)].construction.iter().map(|c| (c.slot, c.turns_remaining)).collect();
    assert_eq!(left, vec![(SlotRef::Slot(1), 2)]);
    let dropped: Vec<&CampaignEvent> = events.iter().filter(|e| e.is_diagnostic()).collect();
    assert_eq!(dropped, vec![&CampaignEvent::ConstructionItemDropped { region: RegionId(10), slot: SlotRef::Slot(99), level_key: "test_building_level".into() }]);
    assert_eq!(dropped[0].script_name(), None);
    assert!(m.end_turn().iter().all(|e| !e.is_diagnostic()), "reported once");
}

/// Cancelling names a queued item by its id, so two cancels issued before the queue is shown
/// again remove exactly those two items (review: by queue position, cancelling A then B removed C).
#[test]
fn recruitment_cancels_name_their_item() {
    let mut m = test_model();
    m.world.factions.get_mut(&A).unwrap().treasury = 10_000;
    for _ in 0..3 {
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    }
    let ids: Vec<RecruitmentItemId> = m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.id).collect();
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2], "{ids:?}");
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: ids[0] }).unwrap();
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: ids[1] }).unwrap();
    let left: Vec<RecruitmentItemId> = m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.id).collect();
    assert_eq!(left, vec![ids[2]]);
    assert_eq!(
        m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: ids[0] }),
        Err(CommandError::UnknownRecruitmentItem(ids[0])),
        "a cancelled item is gone"
    );
}

/// The state hash tells construction slots apart without sentinel indices (review: `Slot(i)` was
/// hashed as `i as u32`, with `u32::MAX` / `u32::MAX - 1` standing for the road and the walls).
#[test]
fn the_state_hash_tells_construction_slots_apart() {
    let m = test_model();
    let hash = |slot| {
        let mut m = m.clone();
        let item = ConstructionItem { slot, level_key: "test_building_level".into(), turns_remaining: 1, cost: 0 };
        m.world.regions.get_mut(&RegionId(10)).unwrap().construction = vec![item];
        m.state_hash()
    };
    let hashes = [hash(SlotRef::Slot(0)), hash(SlotRef::Slot(u32::MAX as usize)), hash(SlotRef::Slot(u32::MAX as usize - 1)), hash(SlotRef::Walls), hash(SlotRef::Road)];
    for (i, a) in hashes.iter().enumerate() {
        assert!(hashes[i + 1..].iter().all(|b| a != b), "slot {i} collides: {hashes:?}");
    }
    // The walls building counts too.
    let mut walled = m.clone();
    walled.world.regions.get_mut(&RegionId(10)).unwrap().fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    assert_ne!(walled.state_hash(), m.state_hash());
}

/// `DemolishBuilding` / `DemolishFort` (exe queue ids `0x84` / `0x89`) and the
/// `CanDemolishBuilding` guard (`0x009B7920`): a standing building of the owner's, nothing queued
/// in the slot. No refund (PROVISIONAL).
#[test]
fn buildings_and_forts_are_demolished() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    // Region 10 slot 0 holds test_building_level: demolishing removes it with no refund.
    assert!(m.can_demolish(RegionId(10), SlotRef::Slot(0)));
    let t = m.world.factions[&A].treasury;
    assert_eq!(m.apply(CampaignCommand::DemolishBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap(), Vec::new());
    assert!(m.world.regions[&RegionId(10)].slots[0].building.is_none());
    assert_eq!(m.world.factions[&A].treasury, t);
    // Nothing left to demolish there; B's region refuses on A's turn.
    assert!(!m.can_demolish(RegionId(10), SlotRef::Slot(0)));
    assert_eq!(
        m.apply(CampaignCommand::DemolishBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }),
        Err(CommandError::CannotBuild("nothing to demolish in that slot".into()))
    );
    assert_eq!(
        m.apply(CampaignCommand::DemolishBuilding { region: RegionId(11), slot: SlotRef::Slot(0) }),
        Err(CommandError::NotYourTurn(B))
    );
    assert_eq!(
        m.apply(CampaignCommand::DemolishBuilding { region: RegionId(99), slot: SlotRef::Slot(0) }),
        Err(CommandError::UnknownRegion(RegionId(99)))
    );
    // A slot with queued work refuses while it is busy: damage slot 0, queue its repair,
    // then demolish is blocked until the repair item is cancelled.
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building =
        Some(BuildingRef { level_key: "test_building_level".into(), health: 40 });
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert!(!m.can_demolish(RegionId(10), SlotRef::Slot(0)), "being repaired");
    assert_eq!(
        m.apply(CampaignCommand::DemolishBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }),
        Err(CommandError::CannotBuild("nothing to demolish in that slot".into()))
    );
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert!(m.can_demolish(RegionId(10), SlotRef::Slot(0)));
    // Fort path (exe `DemolishFort`): build walls, finish them, demolish via SlotRef::Walls.
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 1000, turns: 2, ..Default::default() });
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).unwrap();
    for _ in 0..8 {
        if m.world.regions[&RegionId(10)].fortification.is_some() {
            break;
        }
        m.end_turn();
    }
    assert_eq!(m.world.regions[&RegionId(10)].fortification.as_ref().map(|b| b.level_key.as_str()), Some("test_walls"));
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.turn.humans = vec![A];
    let t = m.world.factions[&A].treasury;
    assert!(m.can_demolish(RegionId(10), SlotRef::Walls));
    m.apply(CampaignCommand::DemolishBuilding { region: RegionId(10), slot: SlotRef::Walls }).unwrap();
    assert!(m.world.regions[&RegionId(10)].fortification.is_none());
    assert_eq!(m.world.factions[&A].treasury, t, "PROVISIONAL: no refund");
    assert!(!m.can_demolish(RegionId(10), SlotRef::Walls));
}

/// The walls card's options: the land option rule (`0x00B43300`) on the fortification slot, an
/// ordinary building slot in the exe (CONFIRMED, `0x00A01F50` lists it with the other slots).
#[test]
fn the_walls_slot_options_follow_the_land_rule() {
    let walls = |m: &CampaignModel| -> Vec<String> { m.construction_options(RegionId(10), SlotRef::Walls).into_iter().map(|o| o.level_key).collect() };
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 1000, turns: 2, ..Default::default() });
    rules.buildings.insert("test_keep".into(), BuildingRules { chain: "test_fort".into(), level: 1, cost: 3000, turns: 3, ..Default::default() });
    rules.buildings.get_mut("test_walls").unwrap().upgrades_to = vec!["test_keep".into()];
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    m.turn = TurnState::in_turn_of(A, vec![A]);
    // An empty fortification slot: the level 0 of the fort chain, nothing else. `test_building_level`
    // is level 0 of `test_chain`, whose slots are `test_slot`, so it is not offered here.
    assert_eq!(walls(&m), vec!["test_walls".to_string()]);
    assert_eq!(
        m.construction_options(RegionId(10), SlotRef::Walls),
        vec![ConstructionOption { level_key: "test_walls".into(), cost: 1000, turns: 2, affordable: true, tech: true }]
    );
    // `affordable` follows the treasury: the owner starts with 1000, so 999 is out of reach.
    m.world.factions.get_mut(&A).unwrap().treasury = 999;
    assert!(!m.construction_options(RegionId(10), SlotRef::Walls)[0].affordable, "999 in the treasury");
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    assert!(m.construction_options(RegionId(10), SlotRef::Walls)[0].affordable);
    assert!(m.construction_options(RegionId(99), SlotRef::Walls).is_empty());
    // While the slot has a construction item there are no options at all.
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).unwrap();
    assert!(walls(&m).is_empty());
    for _ in 0..8 {
        if m.world.regions[&RegionId(10)].fortification.is_some() {
            break;
        }
        m.end_turn();
    }
    // A standing undamaged fort offers only its upgrades, and no new fort.
    assert_eq!(m.world.regions[&RegionId(10)].fortification.as_ref().map(|b| b.level_key.as_str()), Some("test_walls"));
    assert_eq!(walls(&m), vec!["test_keep".to_string()]);
    // A damaged fort has no options at all (the construction panel lists upgrades only at full health).
    m.world.regions.get_mut(&RegionId(10)).unwrap().fortification.as_mut().unwrap().health = 40;
    assert!(walls(&m).is_empty());
    assert!(m.construction_options(RegionId(10), SlotRef::Walls).is_empty());
}

/// The scripts' restricted levels are the model's (`World::restricted_buildings`), and the
/// permission test `0x008BE7F0` rejects them for the options and the construct command alike
/// (`CCQ_BUILDING_CONSTRUCT` `0x00931C80` → `0x00B13D50` starts only a level among the slot's
/// options; CONFIRMED). Review: `BeginConstruction` with a restricted key used to build it.
#[test]
fn a_restricted_level_is_neither_offered_nor_built() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 100, turns: 2, ..Default::default() });
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.restricted_buildings.insert("test_walls".into());
    assert!(m.construction_options(RegionId(10), SlotRef::Walls).is_empty());
    assert!(!m.building_permitted(A, "test_walls"));
    let t = m.world.factions[&A].treasury;
    let r = m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() });
    assert_eq!(r, Err(CommandError::CannotBuild("test_walls".into())));
    assert!(m.world.regions[&RegionId(10)].construction.is_empty());
    assert_eq!(m.world.factions[&A].treasury, t);
    // `remove_restricted_building_level_record` lifts it.
    m.world.restricted_buildings.remove("test_walls");
    assert!(m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).is_ok());
}

/// `FindNextFortUpgradeLevel` (`0x00B430C0`, CONFIRMED) works from the fort's own level index
/// (`+0x188`), never a record lookup of the standing level: the next level is the first record of
/// index + 1, none above the top or when that record is restricted. Review: a standing key missing
/// from the rules used to give no upgrade; the index now comes with the standing level.
#[test]
fn the_map_fort_upgrade_follows_the_level_index() {
    let mut m = test_model();
    let levels = vec![(0, "f0".to_string()), (1, "f1".to_string()), (1, "g1".to_string()), (2, "f2".to_string())];
    assert_eq!(m.map_fort_next_level(0, &levels), Some("f1"));
    assert_eq!(m.map_fort_next_level(1, &levels), Some("f2"));
    assert_eq!(m.map_fort_next_level(2, &levels), None);
    m.world.restricted_buildings.insert("f1".into());
    assert_eq!(m.map_fort_next_level(0, &levels), None, "a restricted record is not passed over for g1");
    // No fort record: the standing level is index 0 of the list, with its index.
    assert_eq!(m.map_fort_standing(RegionId(10), &levels), Some((0, "f0".to_string())));
}
#[test]
fn naval_engagement_and_battle() {
    use super::naval::{engagement, resolve, NavalPair, NavalUnit, NavalVars, ShipRules, ShipState};
    use super::autoresolve::PairResult;
    // Equal ships fire at the same rate: B (second to fire in each step's crew update) never breaks first.
    let p = NavalPair { crew_a: 200.0, crew_b: 200.0, hull_a: 3000.0, hull_b: 3000.0, dmg_a: 0.0, dmg_b: 0.0, sink_a: 0.6, sink_b: 0.6 };
    let s = engagement(&p, 50.0, 80.0, 50.0, 80.0, 0);
    assert_eq!(s.result, PairResult::Draw);
    // A much stronger A wins: B's hull reaches its sink point (1 − 0.6) × 3000 first.
    let s = engagement(&p, 200.0, 80.0, 20.0, 80.0, 0);
    assert_eq!((s.result, s.hull_b), (PairResult::AWins, 1.0));
    let ship = |guns: u32| NavalUnit {
        state: ShipState { crews: [30, 30, 144], max_crews: [30, 30, 144], guns, max_guns: guns, ..Default::default() },
        rules: ShipRules { crews: [30, 30, 144], hull: 3300.0, sink_weight: 0.45, morale: 10.0, fire: [40, 15], guns: 74, range: 3, ..Default::default() },
        human: false,
        force: 0,
    };
    let mut rules = CampaignRules::test_rules();
    for (k, v) in [
        ("autoresolve_gaussian_boundary", 3.0), ("autoresolve_gaussian_standard_deviation", 1.0), ("autoresolve_min_combat_potential_only_win_chance", 0.5),
        ("autoresolve_minimum_win_chance_to_win", 0.225), ("autoresolve_advantage_over_enemy_wipeout_threshold", 0.6), ("autoresolve_stat_massacre_chance", 0.2),
        ("autoresolve_normal_naval_victory_win_percent", 0.16), ("autoresolve_normal_naval_victory_lose_percent", 0.2), ("autoresolve_ship_damage_fuzziness", 0.3),
        ("autoresolve_ship_damage_required_for_sink", 0.875),
    ] {
        rules.variables.insert(k.into(), v);
    }
    let v = NavalVars::from_rules(&rules);
    // Three 74s against one: the big side wins and the lone ship is wiped out (advantage 2/3 > 0.6).
    let mut a = vec![ship(74), ship(74), ship(74)];
    let mut b = vec![ship(74)];
    let mut rng = CaRng::new(7);
    let o = resolve(&mut a, &mut b, &v, &mut rng);
    assert!(o.a_won && o.probabilities[0] >= 0.5 && o.probabilities[0] > o.probabilities[1], "{o:?}");
    assert!(b[0].state.sunk && b[0].state.crew() == 0);
    assert!(a.iter().all(|u| !u.state.sunk));
    // Same inputs, same result.
    let (mut a2, mut b2) = (vec![ship(74), ship(74), ship(74)], vec![ship(74)]);
    assert_eq!(resolve(&mut a2, &mut b2, &v, &mut CaRng::new(7)), o);
    assert_eq!(a2, a);
}

#[test]
fn navies_fight_through_the_model() {
    use super::naval::ShipRules;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.ships.insert("test_ship".into(), ShipRules { crews: [20, 20, 100], hull: 2000.0, sink_weight: 0.45, morale: 9.0, fire: [60, 40], guns: 32, range: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let ship = |id: i32| CampaignUnit { id: UnitId(id), unit_key: "test_ship".into(), men: 140, max_men: 140, character: None, officer_name: Default::default() };
    for (fid, fac, cid, units, x) in [(2000u32, A, 200, vec![ship(50), ship(51), ship(52)], 0), (2001, B, 201, vec![ship(53)], 1)] {
        let mut c = character(cid, fac, CharacterKind::Admiral, 30);
        c.position = pos(x, 0);
        m.world.characters.insert(CharacterId(cid), c);
        m.world.forces.insert(ForceId(fid), MilitaryForce { id: ForceId(fid), faction: fac, commander: Some(CharacterId(cid)), units, is_navy: true });
    }
    // A navy cannot attack an army.
    assert_eq!(m.apply(CampaignCommand::AttackForce { force: ForceId(2000), target: ForceId(1001) }), Err(CommandError::Unsupported("a navy and an army cannot fight")));
    m.apply(CampaignCommand::AttackForce { force: ForceId(2000), target: ForceId(2001) }).unwrap();
    assert!(m.pending_battle.is_some());
    let ev = m.apply(CampaignCommand::Autoresolve).unwrap();
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::BattleCompleted { attacker_won: true, .. })), "{ev:?}");
    // The lone ship was wiped out: its navy is gone; the winners' states are kept.
    assert!(!m.world.forces.contains_key(&ForceId(2001)));
    assert!(m.world.ship_states.contains_key(&UnitId(50)));
    let f = &m.world.forces[&ForceId(2000)];
    assert!(f.units.iter().all(|u| u.men == m.world.ship_states[&u.id].crew()));
}

#[test]
fn spa_looting_turns_the_region_against_the_looter() {
    use super::capture::CaptureChoice;
    let mut m = capture_model(vec![A]);
    let mut rules = (*m.rules).clone();
    rules.features.looting_alignment = Some(("align_pro_french".into(), "align_anti_french".into()));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().religion = "align_pro_french".into();
    m.world.regions.get_mut(&RegionId(11)).unwrap().religions = vec![("align_pro_french".into(), 0.6), ("align_anti_french".into(), 0.4)];
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
    let rel = &m.world.regions[&RegionId(11)].religions;
    assert!((rel[0].1 - 0.1).abs() < 1e-6 && (rel[1].1 - 0.9).abs() < 1e-6, "{rel:?}");
}

/// The looting shift is the campaign's feature, not its key: another campaign names its own two alignments, and a
/// campaign without the feature keeps the shares.
#[test]
fn looting_shifts_the_alignments_a_campaign_names() {
    use super::capture::CaptureChoice;
    for (alignment, expect) in [(Some(("loyal".to_string(), "rebel".to_string())), (0.1, 0.9)), (None, (0.6, 0.4))] {
        let mut m = capture_model(vec![A]);
        let mut rules = (*m.rules).clone();
        rules.campaign = "made_up_campaign".into();
        rules.features.looting_alignment = alignment;
        m.rules = Arc::new(rules);
        m.world.faction_details.entry(A).or_default().religion = "loyal".into();
        m.world.regions.get_mut(&RegionId(11)).unwrap().religions = vec![("loyal".into(), 0.6), ("rebel".into(), 0.4)];
        m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
        m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
        let rel = &m.world.regions[&RegionId(11)].religions;
        assert!((rel[0].1 - expect.0).abs() < 1e-6 && (rel[1].1 - expect.1).abs() < 1e-6, "{rel:?}");
    }
}

#[test]
fn missionaries_spot_hidden_agents() {
    let mut m = agents_model();
    // A's missionary (zeal 3, subterfuge 0) next to B's rake (subterfuge 2); both at (0, 0).
    m.world.characters.insert(CharacterId(106), character(106, A, CharacterKind::CatholicMissionary, 20));
    m.world.characters.insert(CharacterId(107), character(107, B, CharacterKind::Rake, 20));
    agent_details(&mut m, 106, &[], &[("subterfuge", 0), ("research", -1), ("zeal", 3)]);
    agent_details(&mut m, 107, &[], &[("subterfuge", 2)]);
    let mut rules = (*m.rules).clone();
    rules.agent_sight.insert("catholic_missionary".into(), 10);
    m.rules = Arc::new(rules);
    // B's gentleman (no subterfuge, not hidden) is known anyway: never drawn for.
    assert!(super::agents::knows_character(&m, A, CharacterId(105)));
    // Score = rank 3 + 2 (missionary, subterfuge 0) = 5; threshold (5 − 2 + 9) × 5 = 60: spotted on
    // a draw of 60 or more, so some passes spot him and some do not.
    let (mut hits, mut misses) = (0, 0);
    for _ in 0..30 {
        if let Some(d) = m.world.faction_details.get_mut(&A) {
            d.exposed.clear();
        }
        let spotted = m.spotting_pass(A);
        assert!(spotted.iter().all(|&(s, t)| s == CharacterId(106) && t == CharacterId(107)));
        if spotted.is_empty() {
            misses += 1;
        } else {
            hits += 1;
            assert!(super::agents::knows_character(&m, A, CharacterId(107)));
        }
    }
    assert!(hits > 0 && misses > 0, "{hits} {misses}");
    // Once known he is not drawn for again; a rake is no spotter.
    let before = m.rng;
    m.spotting_pass(A);
    if super::agents::knows_character(&m, A, CharacterId(107)) {
        assert_eq!(m.rng, before);
    }
    assert!(m.spotting_pass(B).is_empty() || m.world.characters.values().any(|c| c.faction == B && c.kind == CharacterKind::CatholicMissionary));
}

#[test]
fn hidden_flag_follows_the_stealth_test() {
    let mut m = agents_model();
    // Force 1001 (B, commander 101) in the open, units that cannot hide, no map: not hidden.
    assert!(!m.stealthy(CharacterId(101)));
    // Every unit able to hide: hidden.
    let mut rules = (*m.rules).clone();
    for r in rules.units.values_mut() {
        r.campaign_stealth = true;
    }
    m.rules = Arc::new(rules);
    let has_units = !m.world.forces[&ForceId(1001)].units.is_empty();
    assert_eq!(m.stealthy(CharacterId(101)), has_units);
    m.update_hidden(CharacterId(101));
    assert_eq!(m.world.character_details[&CharacterId(101)].hidden, has_units);
    // Inside a settlement: never.
    m.world.characters.get_mut(&CharacterId(101)).unwrap().garrisoned_in = Some(RegionId(11));
    assert!(!m.stealthy(CharacterId(101)));
    // An agent (no force): never.
    assert!(!m.stealthy(CharacterId(103)));
}

#[test]
fn the_general_pool_refills_and_hires() {
    use super::pool::PoolKind;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let mut rules = (*m.rules).clone();
    rules.general_units.insert(String::new(), "test_unit".into());
    m.rules = Arc::new(rules);
    // An empty pool whose timer is due now: one candidate at the capital, then the timer restarts
    // 4 turns on (no refill bonus).
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    let new = m.pool_tick(A);
    assert_eq!(new.len(), 1);
    let c = new[0];
    assert_eq!(m.world.faction_details[&A].general_pool, (vec![c], elapsed + 4));
    assert_eq!(m.world.characters[&c].position, m.world.regions[&RegionId(10)].settlement.position);
    assert_eq!(m.pool_refill_time(A, PoolKind::General), 4);
    // Not due: nothing.
    assert!(m.pool_tick(A).is_empty());
    // Hiring: 400 + 300 × rank, paid; a new army with the culture's general unit; the timer runs on.
    let cost = m.hire_cost(c).unwrap();
    assert_eq!(cost, 400 + 300 * super::agents::rank(&m, c));
    m.turn.humans = vec![A];
    m.start_campaign();
    let before = m.world.factions[&A].treasury;
    let ev = m.apply(CampaignCommand::HireGeneral { character: c, into: None }).unwrap();
    assert!(matches!(ev[0], CampaignEvent::CharacterHired { cost: k, .. } if k == cost));
    assert_eq!(m.world.factions[&A].treasury, before - cost);
    let f = m.force_of(c).expect("an army");
    assert_eq!(m.world.forces[&f].units[0].unit_key, "test_unit");
    assert!(m.world.faction_details[&A].general_pool.0.is_empty());
    assert!(m.apply(CampaignCommand::HireGeneral { character: c, into: None }).is_err());
}

/// The hire (`0x00A1B8F0`) tests no money and charges `0x00BAF500(cost, 2)` only when the faction is human
/// (+0x6E0): a human hires into debt, an AI hires free.
#[test]
fn a_human_hires_into_debt_and_an_ai_hires_free() {
    let hire = |human: bool| {
        let mut m = test_model();
        m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
        let mut rules = (*m.rules).clone();
        rules.general_units.insert(String::new(), "test_unit".into());
        m.rules = Arc::new(rules);
        let elapsed = m.calendar.turns_elapsed;
        m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
        let c = m.pool_tick(A)[0];
        // Not started: every faction may act (`may_act`), so the AI case needs no AI turn.
        m.turn.humans = if human { vec![A] } else { vec![B] };
        m.world.factions.get_mut(&A).unwrap().treasury = -5;
        let cost = m.hire_cost(c).unwrap();
        m.apply(CampaignCommand::HireGeneral { character: c, into: None }).unwrap();
        (cost, m.world.factions[&A].treasury)
    };
    let (cost, after) = hire(true);
    assert_eq!(after, -5 - cost);
    assert_eq!(hire(false).1, -5);
}

#[test]
fn a_due_historical_character_is_offered_before_a_generic_one() {
    use super::pool::PoolKind;
    use super::rules::HistoricalCandidate;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let mut rules = (*m.rules).clone();
    // Two rows for A's generals: one due in 1805, one not yet; one for B.
    rules.historical = vec![
        HistoricalCandidate { key: "eur_due".into(), male: true, kind: "General".into(), faction: "test_faction_a".into(), years: (1800, 1810), note: String::new() },
        HistoricalCandidate { key: "eur_later".into(), male: true, kind: "General".into(), faction: "test_faction_a".into(), years: (1808, 1812), note: String::new() },
        HistoricalCandidate { key: "eur_other".into(), male: true, kind: "General".into(), faction: "test_faction_b".into(), years: (1800, 1810), note: String::new() },
    ];
    m.rules = Arc::new(rules);
    assert_eq!(m.due_historical(A, PoolKind::General), vec![0]);
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    let c = m.pool_tick(A)[0];
    assert_eq!(m.world.character_details[&c].historical_key.as_deref(), Some("eur_due"));
    assert_eq!(m.world.historical_created, vec!["eur_due".to_string()]);
    // Used up: the next candidate is a generic one (the created list keeps the key).
    assert!(m.due_historical(A, PoolKind::General).is_empty());
    let c2 = m.create_candidate(A, PoolKind::General).unwrap();
    assert_eq!(m.world.character_details[&c2].historical_key, None);
}

#[test]
fn a_general_hired_into_an_army_takes_its_command_there() {
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let mut rules = (*m.rules).clone();
    rules.general_units.insert(String::new(), "test_unit".into());
    m.rules = Arc::new(rules);
    // An army of A under a colonel, 300 units east of the capital.
    let far = pos(m.world.regions[&RegionId(10)].settlement.position.0.to_f32() as i32 + 300, 0);
    let mut col = character(107, A, CharacterKind::Colonel, 25);
    col.position = far;
    m.world.characters.insert(CharacterId(107), col);
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![unit(7)], is_navy: false });
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    let c = m.pool_tick(A)[0];
    // The cost counts the distance of the army, not of the candidate at the capital: 300 / 1000 → +300.
    let at_capital = m.hire_cost(c).unwrap();
    assert_eq!(m.hire_cost_into(c, ForceId(1002)), Some(at_capital + 300));
    m.turn.humans = vec![A];
    m.start_campaign();
    let ev = m.apply(CampaignCommand::HireGeneral { character: c, into: Some(ForceId(1002)) }).unwrap();
    assert!(matches!(ev[0], CampaignEvent::CharacterHired { force: ForceId(1002), .. }));
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CharacterPromoted { .. })));
    let f = &m.world.forces[&ForceId(1002)];
    assert_eq!(f.commander, Some(c));
    assert_eq!(f.units.len(), 2);
    assert_eq!(f.units[1].unit_key, "test_unit");
    assert_eq!(m.world.characters[&c].position, far);
}

#[test]
fn an_admiral_is_hired_onto_a_fleet_and_its_captain_goes() {
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    m.world.characters.insert(CharacterId(108), character(108, A, CharacterKind::Captain, 25));
    let mut ship = unit(8);
    ship.character = Some(CharacterId(108));
    m.world.forces.insert(ForceId(1003), MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(108)), units: vec![ship], is_navy: true });
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed + 100);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed);
    let c = m.pool_tick(A)[0];
    assert_eq!(m.world.characters[&c].kind, CharacterKind::Admiral);
    assert!(m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1000) }).is_err(), "an army is not a fleet");
    m.turn.humans = vec![A];
    m.start_campaign();
    let before = m.world.factions[&A].treasury;
    let ev = m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    let cost = match ev[0] {
        CampaignEvent::CharacterHired { cost, .. } => cost,
        _ => panic!("hired"),
    };
    assert_eq!(m.world.factions[&A].treasury, before - cost);
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(c));
    assert!(!m.world.characters.contains_key(&CharacterId(108)));
    assert!(m.world.faction_details[&A].admiral_pool.0.is_empty());
}

#[test]
fn promotion_in_the_field_makes_the_colonel_a_general_and_fires_character_promoted() {
    use super::effects::SavedBonus;
    use super::portraits::{CulturePortraits, PortraitCategory, PortraitDeck};
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    // Portraits (CHARACTERS_FIDELITY.md §14): A's culture has General decks, none for colonels.
    {
        let rules = Arc::make_mut(&mut m.rules);
        rules.characters.faction_subculture.insert("test_faction_a".into(), "sc".into());
        rules.characters.subculture_culture.insert("sc".into(), "european".into());
        rules.agent_portrait_folders.insert("admiral".into(), "General".into());
    }
    let deck = |order: Vec<u32>| PortraitDeck { count: order.len() as u32, cursor: 0, order, seed: 1 };
    let paths = ["General", "admiral", "colonel", "captain"].map(|a| (a.to_string(), "european".to_string())).to_vec();
    let general = PortraitCategory { key: "General".into(), decks: vec![deck(vec![4, 3]), deck(vec![7]), deck(vec![0, 1]), deck(vec![0])] };
    m.world.portraits = vec![CulturePortraits { culture: "european".into(), paths, categories: vec![general] }];
    let born = |age: i32, m: &CampaignModel| super::details::CharacterDetails { birth: Some(Date { year: (m.calendar.date.year as i32 - age) as u32, ..m.calendar.date }), ..Default::default() };
    let mut col = character(107, A, CharacterKind::Colonel, 25);
    col.position = pos(50, 0);
    m.world.characters.insert(CharacterId(107), col);
    m.world.character_details.insert(CharacterId(107), born(30, &m));
    let mut u = unit(7);
    u.character = Some(CharacterId(107));
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![u, unit(9)], is_navy: false });
    m.turn.humans = vec![A];
    m.start_campaign();
    // Without `promote_general_in_field` the faction cannot promote.
    assert!(m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 0 }).is_err());
    m.world.faction_details.get_mut(&A).unwrap().bonus_base = vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new() }];
    // `bonus_with_difficulty` (faction +0x8D4) wins over `bonus_base` when it is not empty.
    m.world.faction_details.get_mut(&A).unwrap().bonus_with_difficulty = Vec::new();
    // The next number of a European General deck (0 young, 1 old): the turn start's pool
    // candidate has already drawn from the young one.
    let next = |m: &CampaignModel, deck: usize| m.world.portraits[0].categories[0].decks[deck].clone().draw().unwrap();
    let young = next(&m, 0);
    let before = m.world.factions[&A].treasury;
    let ev = m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 0 }).unwrap();
    assert_eq!(ev, vec![CampaignEvent::CharacterPromoted { character: CharacterId(107) }]);
    assert_eq!(m.world.characters[&CharacterId(107)].kind, CharacterKind::General);
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(107)));
    assert!(m.world.factions[&A].treasury < before);
    // The colonel had no portrait; as a General (30) he draws the next young number.
    let p = &m.world.character_details[&CharacterId(107)].portrait;
    assert_eq!((p.index, p.card.clone()), (young as i32, format!("ui/portraits/european/Cards/general/young/{young:03}.tga")));
    // Once: the force has a General now.
    assert!(!m.can_promote_unit(ForceId(1002), 1));
    assert!(m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 1 }).is_err());
    // A naval promotion is free (INFERRED from the static trace; the probe settles it: the naval class's slot +0x44 is a return-0 stub).
    m.world.characters.insert(CharacterId(109), character(109, A, CharacterKind::Captain, 25));
    m.world.character_details.insert(CharacterId(109), born(50, &m));
    let mut ship = unit(11);
    ship.character = Some(CharacterId(109));
    m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: A, commander: Some(CharacterId(109)), units: vec![ship], is_navy: true });
    m.world.faction_details.get_mut(&A).unwrap().bonus_base =
        vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new() }, SavedBonus { kind: 1, bonus: 65, value: 1.0, qualifier: String::new() }];
    m.world.faction_details.get_mut(&A).unwrap().bonus_with_difficulty = Vec::new();
    assert_eq!(m.promotion_cost(ForceId(1005), 0), Some(0));
    let old = next(&m, 1);
    let before = m.world.factions[&A].treasury;
    let ev = m.apply(CampaignCommand::PromoteUnit { force: ForceId(1005), unit: 0 }).unwrap();
    assert!(matches!(ev[0], CampaignEvent::CharacterPromoted { character: CharacterId(109) }));
    assert_eq!(m.world.characters[&CharacterId(109)].kind, CharacterKind::Admiral);
    assert_eq!(m.world.factions[&A].treasury, before, "a naval promotion is free");
    // An admiral (50) draws from the General old deck (agents #6 of admiral is `General`).
    let p = &m.world.character_details[&CharacterId(109)].portrait;
    assert_eq!((p.index, p.card.as_str()), (old as i32, "ui/portraits/european/Cards/general/old/007.tga"));
    // A unit without a character gets a new General made for it.
    let mut u2 = unit(10);
    u2.character = None;
    m.world.forces.insert(ForceId(1004), MilitaryForce { id: ForceId(1004), faction: A, commander: Some(CharacterId(100)), units: vec![u2], is_navy: false });
    m.world.characters.get_mut(&CharacterId(100)).unwrap().kind = CharacterKind::Colonel;
    let young = next(&m, 0);
    let ev = m.apply(CampaignCommand::PromoteUnit { force: ForceId(1004), unit: 0 }).unwrap();
    let new = match ev[0] {
        CampaignEvent::CharacterPromoted { character } => character,
        _ => panic!("promoted"),
    };
    assert_eq!(m.world.characters[&new].kind, CharacterKind::General);
    assert_eq!(m.world.forces[&ForceId(1004)].units[0].character, Some(new));
    assert_eq!(m.world.forces[&ForceId(1004)].commander, Some(new));
    // The new officer is 21..40 (`0x00990EF0` draws his age) and draws the next young number.
    let d = &m.world.character_details[&new];
    let age = m.calendar.date.year as i32 - d.birth.expect("a birth date").year as i32;
    assert!((21..=40).contains(&age), "age {age}");
    assert_eq!((d.portrait.index, d.portrait.info.clone()), (young as i32, format!("ui/portraits/european/Info/general/young/{young:03}.jpg")));
}

#[test]
fn the_interface_gates_and_prices_the_field_promotion() {
    use super::effects::SavedBonus;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let mut col = character(107, A, CharacterKind::Colonel, 25);
    col.position = pos(50, 0);
    m.world.characters.insert(CharacterId(107), col);
    let mut u = unit(7);
    u.character = Some(CharacterId(107));
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![u, unit(9)], is_navy: false });
    // Another faction's turn, no effect, no unit: no.
    assert!(!m.can_promote_unit(ForceId(1002), 0));
    assert!(!m.can_promote_unit(ForceId(1002), 5));
    m.turn.humans = vec![A];
    m.start_campaign();
    assert!(!m.can_promote_unit(ForceId(1002), 0), "no promote_general_in_field yet");
    m.world.faction_details.get_mut(&A).unwrap().bonus_base = vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new() }];
    assert!(m.can_promote_unit(ForceId(1002), 0));
    assert!(m.can_promote_unit(ForceId(1002), 1), "a unit without a character is promoted too");
    // The price the interface shows is the price the promotion charges (PROVISIONAL value).
    let shown = m.promotion_cost(ForceId(1002), 0).expect("a cost");
    assert!(shown > 0, "the shown cost is {shown}");
    assert_eq!(m.promotion_cost(ForceId(1002), 9), None);
    let before = m.world.factions[&A].treasury;
    m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 0 }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, before - shown, "shown == charged");
    // The force has a General now: no second promotion.
    assert!(!m.can_promote_unit(ForceId(1002), 1));
    assert!(m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 1 }).is_err());
    // A unit with no character: the shown price must still be the charged one (the promotion
    // makes him first, and his rank is then -1, which `promotion_cost` assumes).
    let mut col2 = character(110, A, CharacterKind::Colonel, 25);
    col2.position = pos(80, 0);
    m.world.characters.insert(CharacterId(110), col2);
    let mut led = unit(12);
    led.character = Some(CharacterId(110));
    let mut bare = unit(13);
    bare.character = None;
    m.world.forces.insert(ForceId(1006), MilitaryForce { id: ForceId(1006), faction: A, commander: Some(CharacterId(110)), units: vec![led, bare], is_navy: false });
    let shown = m.promotion_cost(ForceId(1006), 1).expect("a cost");
    let before = m.world.factions[&A].treasury;
    m.apply(CampaignCommand::PromoteUnit { force: ForceId(1006), unit: 1 }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, before - shown, "shown == charged for a unit made a character");
}

#[test]
fn the_interface_gate_for_hiring_a_commander_follows_the_pool_and_the_purse() {
    use super::pool::PoolKind;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let mut rules = (*m.rules).clone();
    rules.general_units.insert(String::new(), "test_unit".into());
    m.rules = Arc::new(rules);
    let mut col = character(107, A, CharacterKind::Colonel, 25);
    col.position = pos(50, 0);
    m.world.characters.insert(CharacterId(107), col);
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![unit(7)], is_navy: false });
    m.world.characters.insert(CharacterId(108), character(108, A, CharacterKind::Captain, 25));
    m.world.forces.insert(ForceId(1003), MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(108)), units: vec![unit(8)], is_navy: true });
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    // An empty admiral pool: an army may hire, a fleet may not.
    assert_eq!(m.pool_tick(A).len(), 1);
    assert_eq!(m.world.faction_details[&A].admiral_pool.0.len(), 0);
    m.turn.humans = vec![A];
    m.start_campaign();
    assert!(m.can_recruit_commander(ForceId(1002)), "a General candidate is in the pool");
    assert!(!m.can_recruit_commander(ForceId(1003)), "no admiral candidate");
    assert!(!m.can_recruit_commander(ForceId(9999)), "no such force");
    // The gate tests no money (`0x009D1CD0`): a faction in debt may still open the pool.
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    assert!(m.can_recruit_commander(ForceId(1002)));
    // Another faction's turn.
    m.turn.current = Some(B);
    assert!(!m.can_recruit_commander(ForceId(1002)));
    // With an admiral in the pool the fleet may hire too.
    m.turn.current = Some(A);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed);
    assert_eq!(m.pool_tick(A).len(), 1);
    assert_eq!(m.pool_refill_time(A, PoolKind::Admiral), 4);
    assert!(m.can_recruit_commander(ForceId(1003)));
}

#[test]
fn the_visible_set_grows_during_the_turn_and_is_rebuilt_at_the_turn_end() {
    use super::visibility::{CellSet, Shroud, SightGrid};
    let mut m = test_model();
    m.world.sight_grid = Some(SightGrid::centred(64, 64, 64));
    let empty = CellSet::new(64, 64);
    m.world.shrouds.insert(A, Shroud { explored: empty.clone(), visible: empty.clone(), hidden: empty, active: true });
    m.world.sight_radius.insert(CharacterId(100), 3.0);
    // Off the origin: a character at (0, 0) counts as unplaced and sees nothing.
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(-5, -5);
    m.turn.humans = vec![A];
    m.start_campaign();
    let start = m.compute_visible(A).unwrap();
    assert_eq!(m.world.shrouds[&A].visible, start);
    // The General walks away (south-west, clear of the settlements' discs): what he saw stays
    // visible, what he sees is added.
    let from = m.world.characters[&CharacterId(100)].position;
    m.apply(CampaignCommand::MoveCharacter { character: CharacterId(100), to: pos(from.0.to_f32() as i32 - 15, from.1.to_f32() as i32 - 15) }).unwrap();
    let now = m.compute_visible(A).unwrap();
    assert_ne!(now, start);
    let mid = m.world.shrouds[&A].visible.clone();
    assert!(start.cells().all(|(x, z)| mid.get(x, z)) && now.cells().all(|(x, z)| mid.get(x, z)));
    assert!(mid.len() > now.len());
    // The faction's turn end rebuilds the visible set from the sources alone.
    m.begin_end_turn();
    m.step();
    assert_eq!(m.world.shrouds[&A].visible, now);
    assert!(now.cells().all(|(x, z)| m.world.shrouds[&A].explored.get(x, z)));
    // The fog states across that turn end: a cell he saw only from where he started was Visible
    // during the turn and is Explored after it; one he sees now stays Visible; one nobody has
    // seen stays NeverSeen.
    use super::visibility::FogState;
    let left = start.cells().find(|&(x, z)| !now.get(x, z)).expect("a cell seen only from the start");
    assert!(mid.get(left.0, left.1), "visible during the turn");
    assert_eq!(m.fog_state_at(A, left.0, left.1), FogState::Explored, "left behind: explored after the turn end");
    let here = now.cells().next().expect("a cell in sight now");
    assert_eq!(m.fog_state_at(A, here.0, here.1), FogState::Visible);
    let grid = m.world.sight_grid.unwrap();
    let unseen = (0..grid.rows).flat_map(|z| (0..grid.cols).map(move |x| (x, z))).find(|&(x, z)| !m.world.shrouds[&A].explored.get(x, z)).unwrap();
    assert_eq!(m.fog_state_at(A, unseen.0, unseen.1), FogState::NeverSeen);
}

/// The three fog states a renderer needs, read off the shroud's cell sets (`0x00B7A150`'s test
/// generalised): a cell in `visible` and not in `hidden` is **visible**, one in `explored` but not
/// visible is **explored** (dimmed), anything else **never seen** (black). A faction with no shroud,
/// or with the shroud off, sees everything.
#[test]
fn the_fog_state_tells_never_seen_from_explored_and_visible() {
    use super::visibility::{CellSet, FogState, Shroud, SightGrid};
    let mut m = test_model();
    m.world.sight_grid = Some(SightGrid::centred(64, 64, 64));
    let grid = m.world.sight_grid.unwrap();
    let mut explored = CellSet::new(64, 64);
    let mut visible = CellSet::new(64, 64);
    let mut hidden = CellSet::new(64, 64);
    // A cell never seen, one explored cell, one visible cell and one visible-but-hidden cell.
    explored.set(10, 10);
    visible.set(20, 20);
    hidden.set(30, 30);
    visible.set(30, 30);
    m.world.shrouds.insert(A, Shroud { explored, visible, hidden, active: true });
    let centre = (0.0, 0.0);
    let at = |x: u32, z: u32| ((grid.origin.0 + x as f32 * grid.cell) + grid.cell / 2.0, (grid.origin.1 + z as f32 * grid.cell) + grid.cell / 2.0);
    assert_eq!(m.fog_state(A, centre), FogState::NeverSeen, "nothing seen yet");
    assert_eq!(m.fog_state(A, at(10, 10)), FogState::Explored, "seen once, not now");
    assert_eq!(m.fog_state(A, at(20, 20)), FogState::Visible, "in sight now");
    assert_eq!(m.fog_state(A, at(30, 30)), FogState::NeverSeen, "in `hidden`: the exe's test refuses it");
    assert!(!m.sees(A, at(10, 10)) && m.sees(A, at(20, 20)) && !m.sees(A, at(30, 30)));
    // A cell in both `visible` and `hidden` is not visible; one only in `visible` is.
    m.world.shrouds.get_mut(&A).unwrap().hidden.clear();
    assert_eq!(m.fog_state(A, at(30, 30)), FogState::Visible);
    assert!(m.sees(A, at(30, 30)));
    // Off the grid is never seen; a faction with no shroud (B) and one with the shroud off see all.
    assert_eq!(m.fog_state(A, (1.0e6, 1.0e6)), FogState::NeverSeen);
    assert_eq!(m.fog_state(B, centre), FogState::Visible, "no shroud: the whole map");
    m.world.shrouds.get_mut(&A).unwrap().active = false;
    assert_eq!(m.fog_state(A, centre), FogState::Visible, "the shroud off: the whole map");
    // The per-cell walk agrees with the per-position query, and is absent without a shroud.
    m.world.shrouds.get_mut(&A).unwrap().active = true;
    let states = m.fog_states(A).expect("the states");
    assert_eq!(states.len(), 64 * 64);
    for z in 0..64 {
        for x in 0..64 {
            assert_eq!(states[z as usize * 64 + x as usize], m.fog_state(A, at(x, z)), "cell {x},{z}");
        }
    }
    assert_eq!(m.fog_states(B), None, "a faction with no shroud needs no fog texture");
}

/// The cell-level primitive a terrain renderer walks, and the two properties that make
/// rendering mechanical: it must agree with the per-position query it replaces, and it must
/// cover the **whole** map grid rather than only the regions. `fog_state` / `fog_states` /
/// `fog_state_at` must not be able to disagree, or a cell would be shaded one way and
/// filtered another.
#[test]
fn the_fog_cell_primitive_agrees_with_the_position_query_and_covers_the_whole_map() {
    use super::visibility::{CellSet, FogState, Shroud, SightGrid};
    let mut m = test_model();
    // A grid deliberately not a power of two, and larger than any set we mark, so cells with
    // no region in them are still covered and read as never seen.
    m.world.sight_grid = Some(SightGrid::centred(70, 50, 64));
    let grid = m.world.sight_grid.unwrap();
    let mut explored = CellSet::new(70, 50);
    let mut visible = CellSet::new(70, 50);
    let mut hidden = CellSet::new(70, 50);
    explored.set(3, 4);
    visible.set(60, 45);
    hidden.set(10, 10);
    visible.set(10, 10);
    m.world.shrouds.insert(A, Shroud { explored, visible, hidden, active: true });

    // 1. The cell primitive and the position query agree everywhere, including the marked
    //    cells, an off-grid position and a faction with no shroud.
    for z in 0..grid.rows {
        for x in 0..grid.cols {
            let centre = m.cell_centre(x, z).unwrap();
            // cell_centre must land back inside the cell it names, or the lookup is inconsistent.
            assert_eq!(grid.cell_of(centre), Some((x, z)), "cell_centre({x},{z}) = {centre:?}");
            assert_eq!(
                m.fog_state_at(A, x, z),
                m.fog_state(A, centre),
                "cell {x},{z}: the two paths disagree"
            );
        }
    }
    assert_eq!(m.fog_state_at(A, 3, 4), FogState::Explored);
    assert_eq!(m.fog_state_at(A, 60, 45), FogState::Visible);
    assert_eq!(m.fog_state_at(A, 10, 10), FogState::NeverSeen, "in `hidden`");
    assert_eq!(m.fog_state_at(B, 0, 0), FogState::Visible, "no shroud");
    assert_eq!(m.fog_state_at(A, 999, 999), FogState::NeverSeen, "outside the grid, like an off-map position");

    // 2. Whole-map coverage: every cell of the grid gets a state, and `fog_states` is exactly
    //    the grid in row-major order -- not the regions, and not a bounding box of the marked
    //    cells. Cell (69, 49), the far corner and nowhere near any mark, is in it.
    let states = m.fog_states(A).expect("the states");
    assert_eq!(states.len(), 70 * 50, "the whole grid, one state per cell");
    assert_eq!(states[49 * 70 + 69], FogState::NeverSeen, "the far corner is covered and unseen");
    for z in 0..grid.rows {
        for x in 0..grid.cols {
            assert_eq!(states[z as usize * 70 + x as usize], m.fog_state_at(A, x, z), "cell {x},{z}");
        }
    }

    // 3. `knows` is the labels layer's test and must NOT be `sees`: an explored-but-not-visible
    //    position is still drawn and still labelled, so `knows` is true where `sees` is false.
    let explored_at = m.cell_centre(3, 4).unwrap();
    assert!(m.knows(A, explored_at), "seen once: still labelled");
    assert!(!m.sees(A, explored_at), "...but not currently in sight");
    assert!(m.knows(A, m.cell_centre(60, 45).unwrap()) && m.sees(A, m.cell_centre(60, 45).unwrap()), "in sight now");
    assert!(!m.knows(A, m.cell_centre(0, 0).unwrap()), "never seen: no label");
    assert!(m.knows(B, m.cell_centre(0, 0).unwrap()), "a faction with no shroud knows the whole map");

    // 4. `fog_state == Visible` is exactly `sees` -- on the grid, off it, with and without a
    //    shroud -- so `knows` can never be false where `sees` is true.
    let probes = [m.cell_centre(3, 4).unwrap(), m.cell_centre(60, 45).unwrap(), m.cell_centre(10, 10).unwrap(), (0.0, 0.0), (1.0e6, -1.0e6)];
    for f in [A, B] {
        for p in probes {
            assert_eq!(m.fog_state(f, p) == FogState::Visible, m.sees(f, p), "{f:?} at {p:?}");
            assert!(!m.sees(f, p) || m.knows(f, p), "{f:?} at {p:?}: seen but not known");
        }
    }
    assert!(m.sees(B, (1.0e6, -1.0e6)) && m.knows(B, (1.0e6, -1.0e6)), "no shroud: even off the grid");
    m.world.sight_grid = None;
    assert_eq!(m.cell_centre(0, 0), None, "no grid: no cell, and no panic");
    assert_eq!(m.fog_state(A, (0.0, 0.0)), FogState::Visible, "no grid: like `sees`");
}

#[test]
fn a_spy_network_is_established_after_three_idle_turns() {
    let mut m = agents_model();
    m.world.character_details.get_mut(&CharacterId(103)).unwrap().idle_turns = 3;
    m.turn.humans = vec![A];
    let ev = m.start_campaign();
    assert!(ev.contains(&CampaignEvent::CharacterBuildsSpyNetwork { character: CharacterId(103) }));
    let p = m.world.characters[&CharacterId(103)].position;
    assert_eq!(m.world.network_sight[&A], vec![((p.0.to_f32(), p.1.to_f32()), m.sight_radius(CharacterId(103)))]);
    // Only a human faction's spies; the event only at exactly 3 idle turns, the disc from 3 on.
    assert!(!m.world.network_sight.contains_key(&B));
    m.world.character_details.get_mut(&CharacterId(103)).unwrap().idle_turns = 4;
    assert!(m.spy_network_step(A).is_empty());
    assert_eq!(m.world.network_sight[&A].len(), 1);
    m.world.character_details.get_mut(&CharacterId(103)).unwrap().idle_turns = 2;
    assert!(m.spy_network_step(A).is_empty());
    assert!(m.world.network_sight[&A].is_empty());
}

#[test]
fn a_returning_commander_who_falls_is_reset_and_stays_where_he_fell() {
    let mut m = test_model();
    let d = m.world.character_details.entry(CharacterId(100)).or_default();
    d.returns_after_death = true;
    d.idle_turns = 5;
    d.hidden = true;
    let at = m.world.characters[&CharacterId(100)].position;
    m.character_falls(CharacterId(100));
    let c = &m.world.characters[&CharacterId(100)];
    assert_eq!(c.position, at);
    let d = &m.world.character_details[&CharacterId(100)];
    assert!(!d.hidden && d.idle_turns == 0 && d.returns_after_death);
    assert_ne!(m.world.forces.get(&ForceId(1000)).and_then(|f| f.commander), Some(CharacterId(100)));
}

#[test]
fn turn_end_counters_follow_the_exe() {
    let mut m = agents_model();
    let g = CharacterId(104);
    m.world.regions.get_mut(&RegionId(11)).unwrap().settlement.position = pos(100, 0);
    let home = m.world.regions[&RegionId(10)].settlement.position;
    let away = m.world.regions[&RegionId(11)].settlement.position;
    m.world.characters.get_mut(&g).unwrap().position = home;
    m.turn_end_counters(g);
    let d = &m.world.character_details[&g];
    assert_eq!((d.turns_at_home, d.turns_in_enemy_lands, d.turns_at_sea), (1, 0, 0));
    assert!(d.no_action);
    assert_eq!(d.idle_turns, 1);
    // In the lands of a faction at war with his, having moved: home resets, enemy lands count.
    m.world.set_stance(A, B, Stance::War).unwrap();
    let ch = m.world.characters.get_mut(&g).unwrap();
    ch.position = away;
    ch.movement_points = ch.max_movement_points - 1;
    m.turn_end_counters(g);
    let d = &m.world.character_details[&g];
    assert_eq!((d.turns_at_home, d.turns_in_enemy_lands), (0, 1));
    assert!(!d.no_action);
    assert_eq!(d.idle_turns, 1);
}

#[test]
fn new_characters_fire_character_created() {
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    // A's general pool is due now: the candidate made at A's turn start fires the event.
    let next = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), next);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), next + 100);
    let known: std::collections::BTreeSet<_> = m.world.characters.keys().copied().collect();
    let events = m.end_turn();
    let created: Vec<CharacterId> = events
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::CharacterCreated { character } => Some(*character),
            _ => None,
        })
        .collect();
    let new: Vec<CharacterId> = m.world.characters.keys().filter(|c| !known.contains(c)).copied().collect();
    assert!(!new.is_empty());
    assert_eq!(created, new);
    assert_eq!(CampaignEvent::CharacterCreated { character: new[0] }.script_name(), Some("CharacterCreated"));
}

#[test]
fn ship_potential_from_the_battle_record() {
    use super::naval::{NavalUnit, ShipRules, ShipState};
    // The debugger's values (CAMPAIGN_FIDELITY.md §Naval autoresolve): British 74 (#20 40, #21 25, 74 guns) 7215,
    // 32-gun frigate (60, 40) 4800, merchantman (40, 30, 12 guns) 1260, Indiaman (40, 15, 58 guns) 4785.
    let unit = |fire: [i32; 2], guns: u32, bombard: bool| NavalUnit {
        state: ShipState { guns: guns / 2, max_guns: guns, ..Default::default() },
        rules: ShipRules { fire, guns, bombard, ..Default::default() },
        human: false,
        force: 0,
    };
    assert_eq!(unit([40, 25], 74, false).potential(), 7215.0);
    assert_eq!(unit([60, 40], 32, false).potential(), 4800.0);
    assert_eq!(unit([40, 30], 12, false).potential(), 1260.0);
    assert_eq!(unit([40, 15], 58, false).potential(), 4785.0);
    // Bomb ketches and rocket ships × 1.5; the type's gun count, not the ship's current guns.
    assert_eq!(unit([40, 30], 14, true).potential(), 2205.0);
    // Without a type count, the ship's saved full guns.
    let mut u = unit([40, 25], 74, false);
    u.rules.guns = 0;
    assert_eq!(u.potential(), 7215.0);
}

#[test]
fn sunk_losers_can_be_captured() {
    use super::naval::{resolve, NavalUnit, NavalVars, ShipRules, ShipState};
    let ship = || NavalUnit {
        state: ShipState { crews: [30, 30, 144], max_crews: [30, 30, 144], guns: 74, max_guns: 74, ..Default::default() },
        rules: ShipRules { crews: [30, 30, 144], hull: 3300.0, sink_weight: 0.45, morale: 10.0, fire: [40, 15], guns: 74, range: 3, ..Default::default() },
        human: false,
        force: 0,
    };
    let mut rules = CampaignRules::test_rules();
    for (k, v) in [
        ("autoresolve_gaussian_boundary", 3.0), ("autoresolve_gaussian_standard_deviation", 1.0), ("autoresolve_min_combat_potential_only_win_chance", 0.5),
        ("autoresolve_minimum_win_chance_to_win", 0.225), ("autoresolve_advantage_over_enemy_wipeout_threshold", 0.6), ("autoresolve_stat_massacre_chance", 0.2),
        ("autoresolve_normal_naval_victory_win_percent", 0.16), ("autoresolve_normal_naval_victory_lose_percent", 0.2), ("autoresolve_ship_damage_fuzziness", 0.3),
        ("autoresolve_ship_damage_required_for_sink", 0.875), ("autoresolve_base_best_ship_kills_to_capture", 0.3),
        ("autoresolve_chance_of_not_capturing_ship", 0.0), ("autoresolve_ship_damage_capture_multiplier", 0.35),
    ] {
        rules.variables.insert(k.into(), v);
    }
    let v = NavalVars::from_rules(&rules);
    // Three 74s wipe out a lone 74 (sunk); with no chance of not capturing, the one candidate is taken.
    let mut a = vec![ship(), ship(), ship()];
    let mut b = vec![ship()];
    let o = resolve(&mut a, &mut b, &v, &mut CaRng::new(7));
    assert!(o.a_won);
    assert_eq!(o.captured, vec![(0, 0)]);
    let s = &b[0].state;
    assert!(!s.sunk && s.crew() > 0 && s.crew() <= 204 * 6 / 10, "{s:?}");
    assert!(s.damage.iter().all(|&d| d <= 0.6), "{s:?}");
    // Every sunk ship stays sunk when the roll never passes.
    let mut v2 = v;
    v2.not_capturing = 1.0;
    let (mut a, mut b) = (vec![ship(), ship(), ship()], vec![ship()]);
    let o = resolve(&mut a, &mut b, &v2, &mut CaRng::new(7));
    assert!(o.captured.is_empty() && b[0].state.sunk);
}

/// A world for the government-change tests: A is an absolute monarchy, B a republic, C a constitutional
/// monarchy, every pair has a relationship record with the setup's `government_type` values, and
/// `government_relations` holds a MADE-UP row per pair (the shipped `absolute vs republic` is #2 = -100,
/// #3 = -30: the setup writes #3, a government change shocks it to #2 and lets it recover to #3).
fn government_test_model() -> CampaignModel {
    let mut m = test_model();
    // C holds region 12, so all three are in the game.
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    for (f, key) in [(A, "gov_absolute_monarchy"), (B, "gov_republic"), (C, "gov_constitutional_monarchy")] {
        m.world.factions.get_mut(&f).unwrap().government_key = key.into();
    }
    let mut rules = (*m.rules).clone();
    for (pair, row) in [
        // (government, other government) -> (#2, #3).
        (("gov_absolute_monarchy", "gov_republic"), (-100, -30)),
        (("gov_republic", "gov_absolute_monarchy"), (-40, 10)),
        (("gov_absolute_monarchy", "gov_constitutional_monarchy"), (-50, 30)),
        (("gov_constitutional_monarchy", "gov_absolute_monarchy"), (70, 20)),
        (("gov_republic", "gov_constitutional_monarchy"), (-60, 10)),
        // The one row with #3 below #2: the drift sign follows the pair.
        (("gov_constitutional_monarchy", "gov_republic"), (70, 15)),
        (("gov_republic", "gov_republic"), (-90, -30)),
    ] {
        rules.government_relations.insert((pair.0.into(), pair.1.into()), row);
    }
    m.rules = Arc::new(rules);
    for (a, b) in [(A, B), (B, A), (A, C), (C, A), (B, C), (C, B)] {
        m.setup_relationship_factors(a, b);
    }
    m
}

/// A government change (`0x00B1B5A0`) sets the `government_type` factor of every record that has the
/// faction as its TARGET — the counterpart's attitude towards it — to the new government's #2, with
/// #3 as the limit, drift on, and the sign + unless the limit is below the value; the per-turn update,
/// run by the counterpart, then walks it to #3. The changed faction's own records are not touched
/// (`0x00B1B5A0` is only ever called on the counterpart's record, resolved by `0x00B64C50`).
#[test]
fn government_change_drifts_the_government_type_factor() {
    use super::treaties::slot;
    let mut m = government_test_model();
    let i = slot("government_type");
    // The campaign setup (`0x00B45A40`) writes #3 with no drift and no limit.
    assert_eq!(m.world.relationships[&(A, B)].attitudes[i].value, -30);
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 20);
    assert!(!m.world.relationships[&(A, C)].attitudes[i].limited);
    assert_eq!(m.world.relationships.keys().count(), 6);

    let events = m.apply(CampaignCommand::ChangeGovernment { faction: A, new_government_key: "gov_republic".into() }).unwrap();
    assert!(matches!(&events[..], [CampaignEvent::GovernmentChanged { faction, old_government, new_government }]
        if *faction == A && old_government == "gov_absolute_monarchy" && new_government == "gov_republic"));
    assert_eq!(m.world.factions[&A].government_key, "gov_republic");
    assert_eq!(m.world.factions[&A].government, GovernmentType::Republic);

    // The two records that have A as their target, each with the row of the NEW government
    // (republic towards republic = (-90, -30); constitutional monarchy towards republic = (70, 15)).
    for ((owner, target), (value, limit, drift)) in [((B, A), (-90, -30, 2)), ((C, A), (70, 15, -2))] {
        let f = m.world.relationships[&(owner, target)].attitudes[i];
        // `0x00B1B684` / `0x00B1B686`: value = #2, limit = #3; the drift is +2 unless the limit is below.
        assert_eq!((f.value, f.limit, f.drift, f.limited), (value, limit, drift, true), "{owner:?} {target:?}");
    }
    // The records A owns are left alone: `0x00B1B5A0` is only called on the counterpart's record.
    for (owner, target, value) in [(A, B, -30), (A, C, 30), (B, C, 10), (C, B, 15)] {
        let f = m.world.relationships[&(owner, target)].attitudes[i];
        assert_eq!((f.value, f.drift, f.limited), (value, 0, false), "{owner:?} {target:?}");
    }
    // No record is created for a pair that has none.
    assert_eq!(m.world.relationships.keys().count(), 6);

    // The counterpart's round end walks its own record towards #3...
    m.diplomacy_round_end(B);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[i].value, -88);
    m.diplomacy_round_end(C);
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 68);
    // ...and holds it there once it is on the limit (-90 -> -30 is 30 turns, 70 -> 15 is 28).
    for _ in 0..40 {
        m.diplomacy_round_end(B);
        m.diplomacy_round_end(C);
    }
    assert_eq!(m.world.relationships[&(B, A)].attitudes[i].value, -30);
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 15);
    // A's own round end moves nothing: its records never got a drift.
    assert_eq!(m.world.relationships[&(A, B)].attitudes[i].value, -30);
    assert_eq!(m.world.relationships[&(A, C)].attitudes[i].value, 30);
}

/// A counterpart out of the game still has its record written (`0x00B1B5A0` has no in-the-game test)
/// but never drifts it (`0x00B29170` skips a record whose target is out of the game), and a government
/// the model has no type for, or an unknown faction, is rejected with the model unchanged.
#[test]
fn government_change_skips_the_dead_and_rejects_the_unknown() {
    use super::treaties::slot;
    let mut m = government_test_model();
    let i = slot("government_type");
    // C loses its region and holds no force, so it is out of the game.
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = A;

    assert_eq!(
        m.apply(CampaignCommand::ChangeGovernment { faction: A, new_government_key: "gov_empire".into() }),
        Err(CommandError::Unsupported("government type not modelled"))
    );
    assert_eq!(m.apply(CampaignCommand::ChangeGovernment { faction: FactionId(9), new_government_key: "gov_republic".into() }), Err(CommandError::UnknownFaction(FactionId(9))));
    assert_eq!(m.world.factions[&A].government_key, "gov_absolute_monarchy");

    m.apply(CampaignCommand::ChangeGovernment { faction: A, new_government_key: "gov_republic".into() }).unwrap();
    // B is in the game: written and drifting.
    assert_eq!(m.world.relationships[&(B, A)].attitudes[i].value, -90);
    assert!(m.world.relationships[&(B, A)].attitudes[i].limited);
    m.diplomacy_round_end(B);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[i].value, -88);
    // C is out of the game: written all the same, but its round end moves nothing.
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 70);
    assert!(m.world.relationships[&(C, A)].attitudes[i].limited);
    m.diplomacy_round_end(C);
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 70);
}

/// `0x008BA1E0` (CONFIRMED): units of the four exempt classes (`units` #3 `cavalry_heavy`, `elephants`,
/// `general`, `infantry_elite`; class codes 4 / 0xB / 0xC / 0xE of `0x00EED3E0`) do not desert and draw no
/// random number, so the next unit's loss is the one it would have had alone.
#[test]
fn exempt_unit_classes_do_not_desert() {
    let mut rules = (*test_model().rules).clone();
    for (key, class) in [("test_guard", "infantry_elite"), ("test_cuirassiers", "cavalry_heavy"), ("test_general", "general"), ("test_elephants", "elephants")] {
        let mut u = rules.units["test_unit"].clone();
        u.unit_class = class.into();
        rules.units.insert(key.into(), u);
    }
    let rules = Arc::new(rules);
    let run = |keys: &[&str]| {
        let mut m = test_model();
        m.rules = rules.clone();
        let f = m.world.forces.get_mut(&ForceId(1001)).unwrap();
        f.units = keys.iter().enumerate().map(|(i, k)| CampaignUnit { unit_key: (*k).into(), ..unit(10 + i as i32) }).collect();
        // Income 0: p = 0.3, so each loss depends on the unit's random draw.
        economy::bankrupt_desertion(&mut m, B, 0, 20);
        (m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).collect::<Vec<_>>(), m.rng.unit_float())
    };
    let (alone, next_alone) = run(&["test_unit"]);
    assert!(alone[0] < 80);
    let (mixed, next_mixed) = run(&["test_guard", "test_cuirassiers", "test_general", "test_elephants", "test_unit"]);
    assert_eq!(mixed, vec![80, 80, 80, 80, alone[0]]);
    assert_eq!(next_mixed, next_alone, "the exempt units drew no random number");
}

#[test]
fn population_factors_round_to_hundredths_as_the_exe_stores_them() {
    use super::effects::EffectSet;
    use super::population::{growth_factors, hundredths, FactorInputs};
    // The vanilla saves' stored values: 0.3 → 0.29999998, −0.37 → −0.37, eight hostile units × −0.05 →
    // −0.39999998 (the product of 0.01f and 40 lies exactly between two floats and rounds to even).
    assert_eq!(hundredths(0.3).to_bits(), 0.29999998f32.to_bits());
    assert_eq!(hundredths(-0.37).to_bits(), (-0.37f32).to_bits());
    assert_eq!(hundredths(8.0 * -0.05).to_bits(), (-0.39999998f32).to_bits());
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("baseline_pop_growth".into(), 0.3);
    m.rules = Arc::new(rules);
    let r = m.world.regions[&RegionId(10)].clone();
    let set = EffectSet::default();
    let s = growth_factors(&m, &r, r.population, &r.population_state, &FactorInputs { set: &set, hostile_units: 8 });
    assert_eq!(s.factors[0].to_bits(), 0.29999998f32.to_bits());
    assert_eq!(s.factors[3].to_bits(), (-0.39999998f32).to_bits());
    assert_eq!(s.capacity, r.population_state.base_capacity);
    assert!(!s.overcrowded);
    // Above 90% of the capacity: food shortages (0.9 − r) × 10 × (base + buildings + ports).
    let crowded = growth_factors(&m, &r, r.population_state.base_capacity / 100 * 95, &r.population_state, &FactorInputs { set: &set, hostile_units: 0 });
    assert!(crowded.overcrowded);
    assert_eq!(crowded.factors[4], hundredths((0.9 - 0.95f32) * 10.0 * 0.29999998));
}

#[test]
fn population_grows_by_its_rounded_share_and_keeps_the_minimum() {
    use super::population::{grow, PopulationState};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("minimum_population".into(), 1000.0);
    m.rules = Arc::new(rules);
    let r = m.world.regions[&RegionId(10)].clone();
    // `auto_nr4_t4` → `orig_over_nr4_0252`, eur_east_prussia: 1296529 at −0.07% → 1295621, down.
    let state = PopulationState { factors: [0.29999998, 0.0, -0.37, 0.0, 0.0, 0.0, 0.0], trend: 3, ..Default::default() };
    let (pop, next) = grow(&m, &r, 1_296_529, &state);
    assert_eq!((pop, next.trend), (1_295_621, 3));
    // Never below `minimum_population`; unchanged reads as trend 2.
    assert_eq!(grow(&m, &r, 1000, &state).0, 1000);
    assert_eq!(grow(&m, &r, 1000, &state).1.trend, 2);
    let up = PopulationState { factors: [0.29999998, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
    assert_eq!(grow(&m, &r, 100_000, &up), (100_300, PopulationState { growth: up.factors[0], trend: 1, ..up.clone() }));
}

#[test]
fn religion_normalisation_drops_dust_and_leaves_an_empty_breakdown_alone() {
    use super::population::normalise_religions;
    let mut shares = vec![("a".to_string(), 0.6f32), ("b".to_string(), 0.3), ("c".to_string(), 0.0004)];
    normalise_religions(&mut shares);
    assert_eq!(shares[2].1, 0.0);
    assert!((shares[0].1 - 0.6 / 0.9).abs() < 1e-6 && (shares[1].1 - 0.3 / 0.9).abs() < 1e-6);
    // ORIGINAL BUG (0x00AA4860): the exe writes the first share of an empty breakdown through a null list.
    let mut empty: Vec<(String, f32)> = Vec::new();
    normalise_religions(&mut empty);
    assert!(empty.is_empty());
    let mut zero = vec![("a".to_string(), 0.0f32), ("b".to_string(), 0.0)];
    normalise_religions(&mut zero);
    assert_eq!((zero[0].1, zero[1].1), (1.0, 0.0));
}

#[test]
fn the_round_end_refreshes_then_grows_every_region() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("baseline_pop_growth".into(), 0.3);
    m.rules = Arc::new(rules);
    let before = m.world.regions[&RegionId(10)].population;
    m.end_turn();
    m.end_turn();
    m.end_turn();
    let r = &m.world.regions[&RegionId(10)];
    assert!(r.population > before, "{} -> {}", before, r.population);
    assert_eq!(r.population_state.trend, 1);
    assert_eq!(r.population_state.factors[0].to_bits(), 0.29999998f32.to_bits());
}

#[test]
fn town_wealth_growth_breaks_down_by_factor_and_predicts_constructions() {
    use super::economy::{region_wealth, wealth_trend};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.get_mut("test_building_level").unwrap().effects.push(("tw_growth_education".into(), 7.0));
    rules.buildings.get_mut("test_building_level_2").unwrap().effects.push(("tw_growth_roads".into(), 30.0));
    m.rules = Arc::new(rules);
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    r.discontent_growth = -4;
    let reg = r.clone();
    let now = region_wealth(&m, None, &reg, false);
    assert_eq!(now.factors[0], 7, "{now:?}");
    assert_eq!(now.factors[9], -4);
    assert_eq!(now.growth, super::economy::recompute_region(&m, &reg).1);
    // Predicted: the level under construction in the slot's place (the education building upgraded to roads).
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    r.construction.push(ConstructionItem { slot: SlotRef::Slot(0), level_key: "test_building_level_2".into(), turns_remaining: 2, cost: 600 });
    let reg = r.clone();
    let next = region_wealth(&m, None, &reg, true);
    assert_eq!((next.factors[0], next.factors[4]), (0, 30), "{next:?}");
    assert_eq!(region_wealth(&m, None, &reg, false), now);
    // The trend codes of 0x00AB4410.
    assert_eq!([21, 20, 1, 0, -1, -20, -21].map(wealth_trend), [0, 1, 1, 2, 3, 3, 4]);
}

/// `0x00C5C040`: a faction offers every region it owns but its capital.
#[test]
fn tradeable_regions_leave_out_the_capital() {
    let mut m = test_model();
    let owned: Vec<RegionId> = m.world.regions.values().filter(|r| r.owner == A).map(|r| r.id).collect();
    assert!(owned.contains(&RegionId(10)) && owned.len() > 1, "{owned:?}");
    m.world.faction_details.entry(A).or_default().capital = None;
    assert_eq!(m.tradeable_regions(A).collect::<Vec<_>>(), owned);
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let without: Vec<RegionId> = owned.iter().copied().filter(|r| *r != RegionId(10)).collect();
    assert_eq!(m.tradeable_regions(A).collect::<Vec<_>>(), without);
}

/// The deal's region and technology records (UI_FIDELITY.md §4.9): `ProposeRegions` replaces the
/// regions record (`0x00933CC0` → `0x00C4B0A0`), two empty lists leave it, `clear` empties it;
/// `ProposeTechnologies` (`0x009340A0`) clears on two empty lists too; an unknown region or
/// technology aborts; nothing without a negotiation.
#[test]
fn deal_records_follow_the_propose_executors() {
    use super::rules::TechRules;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![] });
    m.rules = Arc::new(rules);
    let regions = |m: &CampaignModel| m.negotiations.current.as_ref().unwrap().regions.clone();
    let techs = |m: &CampaignModel| m.negotiations.current.as_ref().unwrap().technologies.clone();
    let propose = |clear, demanded: &[u32], offered: &[u32]| CampaignCommand::ProposeRegions {
        clear,
        demanded: demanded.iter().map(|&r| RegionId(r)).collect(),
        offered: offered.iter().map(|&r| RegionId(r)).collect(),
    };
    assert_eq!(m.apply(propose(false, &[11], &[])), Err(CommandError::NoNegotiation));
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(propose(false, &[11], &[12])).unwrap();
    assert_eq!((regions(&m).demanded, regions(&m).offered), (vec![RegionId(11)], vec![RegionId(12)]));
    // Replaced, not added to.
    m.apply(propose(false, &[], &[10])).unwrap();
    assert_eq!((regions(&m).demanded, regions(&m).offered), (vec![], vec![RegionId(10)]));
    // Two empty lists leave the regions record; an unknown region aborts.
    m.apply(propose(false, &[], &[])).unwrap();
    assert_eq!(regions(&m).offered, vec![RegionId(10)]);
    assert_eq!(m.apply(propose(false, &[99], &[])), Err(CommandError::UnknownRegion(RegionId(99))));
    assert_eq!(regions(&m).offered, vec![RegionId(10)]);
    m.apply(propose(true, &[11], &[])).unwrap();
    assert!(regions(&m).is_empty());
    // Technologies: an unknown key aborts; two empty lists clear.
    let t = |demanded: &[&str], offered: &[&str]| CampaignCommand::ProposeTechnologies {
        clear: false,
        demanded: demanded.iter().map(|s| s.to_string()).collect(),
        offered: offered.iter().map(|s| s.to_string()).collect(),
    };
    m.apply(t(&["admin1_a"], &[])).unwrap();
    assert_eq!(techs(&m).demanded, vec!["admin1_a".to_string()]);
    assert_eq!(m.apply(t(&["nope"], &[])), Err(CommandError::UnknownTechnology("nope".into())));
    assert_eq!(techs(&m).demanded, vec!["admin1_a".to_string()]);
    m.apply(t(&[], &[])).unwrap();
    assert!(techs(&m).is_empty());
    // Clear empties both records; a new negotiation starts empty.
    m.apply(propose(false, &[11], &[])).unwrap();
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert!(regions(&m).is_empty());
    m.apply(propose(false, &[11], &[])).unwrap();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    assert!(regions(&m).is_empty());
}

/// Accepting the deal (`0x00C114B0`): the regions record's demanded regions go to the proposer and
/// the offered ones to the recipient (`0x00C18BF0` → `0x00B449F0`): queues cleared, research at
/// the region's schools stopped; no army moves (CONFIRMED chain): the old owner's army stays
/// inside, still selectable, and the new owner's army entering does not merge into it.
#[test]
fn accepted_regions_change_hands_and_keep_the_old_garrison() {
    use super::details::TechResearch;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    // Region 12 (A) holds A's army and A researches at its school (slot id 55).
    m.world.regions.get_mut(&RegionId(12)).unwrap().garrison = Some(ForceId(1000));
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(12));
    m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(RegionSlot {
        key: "school".into(),
        slot_type: "test_slot".into(),
        building: None,
        position: None,
        port: false,
        holder: None,
        id: 55,
    });
    m.world.faction_details.entry(A).or_default().research.insert("t".into(), TechResearch { progress: 4.0, researcher: 55, traded: 0 });
    m.world.regions.get_mut(&RegionId(11)).unwrap().recruitment_queue.push(RecruitmentItem {
        id: RecruitmentItemId(9),
        unit_key: "u".into(),
        turns_remaining: 2,
        cost: 0,
        target: None,
    });
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![RegionId(12)] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert_eq!(m.world.regions[&RegionId(12)].owner, B);
    assert!(m.world.regions[&RegionId(11)].recruitment_queue.is_empty());
    assert_eq!(m.world.faction_details[&A].research["t"], TechResearch { progress: 4.0, researcher: 0, traded: 0 });
    // A's army stays inside B's settlement, linked as before (visible and selectable).
    assert_eq!(m.world.regions[&RegionId(12)].garrison, Some(ForceId(1000)));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, Some(RegionId(12)));
    assert!(m.force_position(ForceId(1000)).is_some());
    // B's army marching into its new settlement does not merge into A's army (PROVISIONAL: waits outside).
    let units_a = m.world.forces[&ForceId(1000)].units.len();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = m.world.regions[&RegionId(12)].settlement.position;
    let _ = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1001), region: RegionId(12) });
    assert!(m.world.forces.contains_key(&ForceId(1001)));
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), units_a);
    assert_eq!(m.world.forces[&ForceId(1000)].faction, A);
    // Accepting again changes nothing (the regions are already theirs: 0x00A64AC0 returns).
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!((m.world.regions[&RegionId(11)].owner, m.world.regions[&RegionId(12)].owner), (A, B));
    // No negotiation, no deal.
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::NoNegotiation));
}

/// Accepted technologies (`0x00C18CF0`): the receiver researches each one it has at state 1..4
/// (`0x008CDCB0` → `0x008EED20`: progress = cost, no school, then availability), the giver's
/// traded count (+0x28, `0x008F3DD0`) goes up by one per technology handed over, granted or not.
#[test]
fn accepted_technologies_are_granted_and_counted() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![] });
    rules.technologies.insert("mil1".into(), TechRules { cost: 40, building_level: "test_building_level".into(), requires: vec![] });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin1_a".into(), state::RESEARCHED), ("mil1".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies = vec![("admin1_a".into(), state::AVAILABLE), ("mil1".into(), state::RESEARCHED)];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["mil1".into()], offered: vec!["admin1_a".into()] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.tech_state(B, "admin1_a"), Some(state::RESEARCHED));
    assert_eq!(m.world.faction_details[&B].research["admin1_a"].progress, 25.0);
    assert_eq!(m.tech_state(A, "mil1"), Some(state::RESEARCHED));
    assert_eq!(m.world.faction_details[&A].research["mil1"].progress, 40.0);
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].traded, 1);
    assert_eq!(m.world.faction_details[&B].research["mil1"].traded, 1);
    // A deal applies once: accepted again (ProposeDeal then AcceptOffer), nothing changes; after
    // the deal is proposed anew it applies again (a granted technology is not granted twice, but
    // the giver's count goes up per hand-over, as 0x008F3DD0).
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].traded, 1);
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec![], offered: vec!["admin1_a".into()] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].traded, 2);
    // A technology the receiver has no entry for is not granted.
    assert!(!m.grant_technology(C, "admin1_a"));
}

/// The technology record's value for the AI recipient (`0x00C13360` → `0x00A36B20`): each item
/// from the model's costs, holders and the proposer's traded counts, summed, × trunc(inflation).
#[test]
fn technology_deal_value_reads_the_model() {
    use super::deal_value::{technology_value, DealEvaluation};
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A];
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![] });
    rules.technologies.insert("mil1".into(), TechRules { cost: 40, building_level: "test_building_level".into(), requires: vec![] });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin1_a".into(), state::RESEARCHED), ("mil1".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies = vec![("admin1_a".into(), state::AVAILABLE), ("mil1".into(), state::RESEARCHED)];
    m.world.faction_details.entry(A).or_default().research.entry("admin1_a".into()).or_default().traded = 1;
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["mil1".into()], offered: vec!["admin1_a".into()] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    // One holder each (doubled); the offered one divided by (1 + 1)²; inflation 2.5 counts as 2.
    let v = m.technology_deal_value(&n, 2.5);
    let gain = technology_value(25, 1, 1, true, true).gain;
    let cost = technology_value(40, 1, 0, false, true).cost;
    assert_eq!((gain, cost), ((500 + 344) * 2 / 4, (500 + 578) * 2));
    assert_eq!((v.gain, v.cost, v.given), (gain * 2, cost * 2, cost * 2));
    // A dear technology demanded for a cheap one: not fair.
    let mut e = DealEvaluation::default();
    e.add(5, v, 0.0, 1.0);
    assert!(!e.fair());
}

/// The technology goals of the AI's two goal lists (`0x008F4F10`): the giver has it researched and
/// the receiver has it in state 1, 2 or 3; the weight of one the AI gives follows its spread.
#[test]
fn technology_goals_follow_the_research_states() {
    use super::research::state;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("t1".into(), state::RESEARCHED), ("t2".into(), state::RESEARCHED), ("t3".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies =
        vec![("t1".into(), state::AVAILABLE), ("t2".into(), state::UNAVAILABLE), ("t3".into(), state::RESEARCHED)];
    assert!(m.is_technology_goal("t1", A, B), "researched by the giver, available to the receiver");
    assert!(!m.is_technology_goal("t2", A, B), "not yet available to the receiver (state 4)");
    assert!(!m.is_technology_goal("t1", B, A), "the giver must have it researched");
    assert!(m.is_technology_goal("t3", B, A));
    // Fewer than 8 factions: t = 1, so any holder makes the spread 1 and the weight −0.5.
    assert_eq!(m.given_technology_goal_weight("t1"), -0.5);
    assert_eq!(m.given_technology_goal_weight("nobody_has_it"), -1.0);
}

/// The AI evaluates a deal of technologies as the exe's `0x00AA5ED0` (AI_RESEARCH.md §4): value,
/// goal weights, `fair` / `good`, and the `diplomacy_options` refusal.
#[test]
fn the_ai_evaluates_technology_deals() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A];
    let mut rules = (*m.rules).clone();
    let tech = |cost| TechRules { cost, building_level: "test_building_level".into(), requires: vec![] };
    rules.technologies.insert("admin1_a".into(), tech(1000));
    rules.technologies.insert("military1_b".into(), tech(1000));
    rules.technologies.insert("economy1_c".into(), tech(100));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("admin1_a".into(), state::RESEARCHED), ("military1_b".into(), state::AVAILABLE), ("economy1_c".into(), state::RESEARCHED)];
    m.world.faction_details.entry(B).or_default().technologies =
        vec![("admin1_a".into(), state::AVAILABLE), ("military1_b".into(), state::RESEARCHED), ("economy1_c".into(), state::AVAILABLE)];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    // Nothing proposed: nothing to answer.
    assert_eq!(m.ai_accepts_deal(), None);
    // An even trade (both worth 20452 × 2, one holder each): the AI's need weight for the admin
    // technology (≤ 0.5 × 500) and the spread weight of the one it gives (−0.5 × 500, one holder of
    // two factions) leave it fair.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["military1_b".into()], offered: vec!["admin1_a".into()] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(true));
    assert!(!m.ai_refuses_deal());
    // A cheap technology for a dear one: refused.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["military1_b".into()], offered: vec!["economy1_c".into()] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(false));
    assert!(m.ai_refuses_deal());
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::DealRefused));
    // A gift is accepted.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec![], offered: vec!["economy1_c".into()] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(true));
    // The AI's diplomacy options forbid accepting technology deals from A (1 or 3): refused.
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[5] = 1;
    assert_eq!(m.ai_accepts_deal(), Some(false));
    // A human recipient answers for itself.
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[5] = 0;
    m.turn.humans = vec![A, B];
    assert_eq!(m.ai_accepts_deal(), None);
}

/// `force_diplomacy` permissions have one home, the model's relationship (`0x00B28670`): set for
/// one direction only, refused for the same faction (the original's `0x00B64C50` miss bug), an
/// unknown faction or option, or a pair with no relationship (never created there); the
/// negotiation panel and the deal rules read them back.
#[test]
fn diplomacy_permissions_live_in_the_model() {
    use super::negotiation::NegotiationAction as N;
    let mut m = test_model();
    let (peace, war) = (N::Peace.option(), N::War.option());
    m.relationship_mut(A, B);
    m.relationship_mut(B, A);
    assert!(!m.world.relationships.contains_key(&(A, C)));
    assert!(!m.set_diplomacy_option(A, C, war, false, false), "no relationship: refused");
    assert!(!m.world.relationships.contains_key(&(A, C)), "and none created");
    assert!(m.may_propose(A, B, war) && m.may_accept(A, B, war), "nothing stored: allowed");
    assert!(m.set_diplomacy_option(A, B, war, false, true));
    assert_eq!((m.diplomacy_option(A, B, war), m.diplomacy_option(B, A, war)), (2, 0));
    assert!(!m.may_propose(A, B, war) && m.may_accept(A, B, war));
    assert!(m.negotiation_actions(A, B).iter().any(|r| r.action == N::War && r.forbidden));
    assert!(m.negotiation_actions(B, A).iter().all(|r| !r.forbidden), "the other direction is untouched");
    assert!(!m.deal_goal_allowed(A, B, war) && m.deal_goal_allowed(B, A, war));
    assert!(m.set_diplomacy_option(B, A, peace, true, false));
    assert!(!m.may_accept(B, A, peace) && !m.deal_goal_allowed(A, B, peace));
    assert!(!m.set_diplomacy_option(A, A, war, false, false));
    assert!(!m.set_diplomacy_option(A, FactionId(999), war, false, false));
    assert!(!m.set_diplomacy_option(A, B, 14, false, false));
    assert!(!m.world.relationships.contains_key(&(A, A)));
    assert!(m.may_propose(A, B, 14), "an index past the table is allowed");
}

/// The research need from the model: enemies / allies, forts and forces (`0x00ABB340`).
#[test]
fn the_research_need_reads_the_model() {
    let mut m = test_model();
    // No history (16), no enemies or allies (M = 1, D = 1), p1 / p2 from B's forces.
    let v = m.research_need(B);
    let land = m.world.forces.values().filter(|f| f.faction == B && !f.is_navy && !f.units.is_empty()).count() as u32;
    let naval = m.world.forces.values().filter(|f| f.faction == B && f.is_navy && !f.units.is_empty()).count() as u32;
    let ports = m.world.regions.values().filter(|r| r.owner == B).map(|r| r.slots.iter().filter(|s| s.port).count() as u32).sum::<u32>();
    let p1 = deal_value::need_power(land >> 2);
    let p2 = deal_value::need_power((ports >> 1) + (naval >> 2));
    assert_eq!(v, deal_value::research_need_values(1, 1, 16, p1, p2));
    // At war with A: one enemy → M = 2.
    m.world.set_stance(A, B, Stance::War).unwrap();
    let v = m.research_need(B);
    assert_eq!(v, deal_value::research_need_values(2, 1, 16, p1, p2));
}

/// The AI evaluates a deal of regions as the exe's `0x00C131C0` → `0x00A364B0` → `0x00AA1E90`
/// (AI_RESEARCH.md §4 "Region value"): the region goals (−1 × 2500 each), the worth of each
/// region to each side, the human-demand factor and its counter, and the `diplomacy_options`
/// refusal. Bases from the formula: region 10 (gdp 4000) 35000, 11 (8000) 55000, 12 (2000)
/// 25000.
#[test]
fn the_ai_evaluates_region_deals() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.region_neighbours =
        [(RegionId(10), vec![RegionId(11)]), (RegionId(11), vec![RegionId(10), RegionId(12)]), (RegionId(12), vec![RegionId(11)])].into();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    assert_eq!(m.ai_accepts_deal(), None, "an empty deal");
    // A gift bordering the AI: worth 25000 to B (no slots, B holds a region), against the region
    // goal's −2500: accepted.
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![], offered: vec![RegionId(12)] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 25000, cost: 0, given: 0 });
    assert_eq!(m.ai_accepts_deal(), Some(true));
    // A gift that borders none of the AI's regions counts too (attitude ≥ 0 needs no border).
    let neighbours = std::mem::take(&mut m.world.region_neighbours);
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 25000, cost: 0, given: 0 });
    assert_eq!(m.ai_accepts_deal(), Some(true));
    m.world.region_neighbours = neighbours;
    // A demand for B's only region: 55000 to A (A has another region with slots); to B ×2 (its
    // only region in the theatre) and ×1.5 (its last with slots) = 165000. Refused.
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 0, cost: 55000, given: 165000 });
    assert_eq!(m.region_deal_value(&n, 2.5), deal_value::DealValue { gain: 0, cost: 110000, given: 330000 }, "inflation ×2");
    assert!(m.ai_refuses_deal());
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::DealRefused));
    assert_eq!(m.world.regions[&RegionId(11)].owner, B);
    // A swap that pays: region 10 stored at 200000, region 11 at 10000.
    m.world.region_base_values = [(RegionId(10), 200_000), (RegionId(11), 10_000)].into();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![RegionId(10)] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 200_000, cost: 10_000, given: 30_000 });
    // The AI's diplomacy options forbid region deals from A (1 or 3): refused.
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[4] = 3;
    assert_eq!(m.ai_accepts_deal(), Some(false));
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[4] = 0;
    assert_eq!(m.ai_accepts_deal(), Some(true));
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!((m.world.regions[&RegionId(10)].owner, m.world.regions[&RegionId(11)].owner), (B, A));
    // The human receiver counts its deal region (faction +0x938); the AI receiver does not.
    assert_eq!(m.world.deal_regions_received.get(&A), Some(&1));
    assert_eq!(m.world.deal_regions_received.get(&B), None);
    // So A's next demand counts m = 1 + 1: B's worth of region 10 (200000 × 2 × 1.5) × 1.5.
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(10)], offered: vec![] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 0, cost: 200_000, given: 900_000 });
    // An AI proposer's demands are not scaled.
    m.turn.humans = vec![];
    assert_eq!(m.region_deal_value(&n, 1.0).given, 600_000);
    // A human recipient answers for itself.
    m.turn.humans = vec![A, B];
    assert_eq!(m.ai_accepts_deal(), None);
    assert!(!m.ai_refuses_deal());
}

/// A deal of regions and technologies is evaluated as one sum (records 4 then 5).
#[test]
fn the_ai_evaluates_regions_and_technologies_together() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.region_neighbours = [(RegionId(12), vec![RegionId(11)]), (RegionId(11), vec![RegionId(12)])].into();
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("military1_b".into(), TechRules { cost: 1000, building_level: "test_building_level".into(), requires: vec![] });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("military1_b".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies = vec![("military1_b".into(), state::RESEARCHED)];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    // A technology worth 20452 × 2 (one holder) for nothing: refused.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["military1_b".into()], offered: vec![] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(false));
    // With region 12 (25000 to B, −2500 region goal, −250 technology goal) added, still short.
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![], offered: vec![RegionId(12)] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(false));
    // Region 12 stored at 60000: 1.05 × (60000 − 2500 − 250) against 40904 → accepted.
    m.world.region_base_values.insert(RegionId(12), 60_000);
    assert_eq!(m.ai_accepts_deal(), Some(true));
}

/// The world for the commander recruitment tests: A's general 100 (army 1000) stands at (20, 0); A owns
/// region 10 (settlement at (10, 0), the fixture building: `test_unit` / `test_recruit`, 2 recruitment
/// points) and a new region 13 at (13, 0) with the same building, so 13 is the nearer source. Without a
/// map the path cost is the straight line at the off-road cost.
fn commander_recruitment_model() -> CampaignModel {
    let mut m = test_model();
    let r = region(13, A, 1000, 1);
    m.world.regions.insert(r.id, r);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(20, 0);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m
}

fn option<'a>(r: &'a super::CommanderRecruitment, unit: &str) -> &'a super::CommanderOption {
    r.options.iter().find(|o| o.unit_key == unit).unwrap_or_else(|| panic!("no {unit} option: {r:?}"))
}

/// `0x00B73030`: the wait for a training place, from the queue's turns left.
#[test]
fn the_queue_wait_counts_down_the_training_places() {
    use super::commander_recruitment::queue_wait;
    // Below capacity: no wait; at capacity: the shortest item.
    assert_eq!(queue_wait(2, &[]), 0);
    assert_eq!(queue_wait(2, &[5]), 0);
    assert_eq!(queue_wait(2, &[5, 3]), 3);
    // Above: the first two count down; after 2 turns the 2 leaves and the queue is at capacity (4 - 2, 1).
    assert_eq!(queue_wait(2, &[4, 2, 1]), 2 + 1);
    // Several leave at once and the queue drops below capacity.
    assert_eq!(queue_wait(2, &[1, 1, 3]), 1);
    // Only the first `capacity` items count down: the 9 behind does not until it is first.
    assert_eq!(queue_wait(1, &[2, 9]), 2 + 9);
}

/// A commander's options (`0x00B72DF0`): each unit once, from the source with the shortest training plus
/// march; the march is the path cost over `units` #9; a farther source only competes when it is the first
/// (the search for the next ones is cut at the first one's path cost).
#[test]
fn a_commander_recruits_each_unit_from_its_quickest_source() {
    let mut m = commander_recruitment_model();
    let speed = m.rules.units["test_recruit"].travel_speed as f32;
    let r = m.commander_recruitment(A, CharacterId(100));
    assert_eq!(r.options.len(), 2, "{r:?}");
    let o = option(&r, "test_recruit");
    // Region 10 comes first (merge order: equal flags and cost, lower region); 13 is nearer and as quick.
    assert_eq!(o.region, RegionId(13));
    assert_eq!(o.training_turns, m.rules.units["test_recruit"].turns as i32);
    assert!((o.travel_turns - 7.0 / speed).abs() < 1e-6, "{o:?}");
    assert!(o.available());
    // A busy queue in 13 (both places taken for 5 turns): 10's 2 + 10/30 turns beat 13's 5 + 2 + 7/30.
    for id in [9100, 9101] {
        m.world.regions.get_mut(&RegionId(13)).unwrap().recruitment_queue.push(RecruitmentItem {
            id: RecruitmentItemId(id),
            unit_key: "test_unit".into(),
            turns_remaining: 5,
            cost: 0,
            target: None,
        });
    }
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.training_turns), (RegionId(10), 2), "{o:?}");
    assert!((o.travel_turns - 10.0 / speed).abs() < 1e-6, "{o:?}");
}

/// ORIGINAL BUG (`0x00B68FE0` over `0x00B61D50`): a source whose queue trains nothing (no recruitment
/// points) has no training estimate (-1), which the exe takes as the shortest time, so its queue would get
/// the item and never train it. Ours ranks it after every source that can train.
#[test]
fn a_source_without_recruitment_points_is_not_chosen() {
    let mut m = commander_recruitment_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_no_points".into(), BuildingRules { chain: "test_chain_2".into(), units_allowed: vec!["test_recruit".into()], ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(13)).unwrap().slots[0].building = Some(BuildingRef { level_key: "test_no_points".into(), health: 100 });
    assert_eq!(m.recruitment_points(RegionId(13), false), 0);
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.training_turns), (RegionId(10), 2), "{o:?}");
}

/// The commander's queue is the items queued for him (item +0x18, `0x00B26020`); ten of them refuse every
/// option as queue-full (`0x00B1BBB0`), whatever the source queues hold. The recruit command keeps the
/// target and the panel names the source region as the card's manager.
#[test]
fn items_queued_through_a_commander_are_his_queue() {
    use super::commands::ENTRY_QUEUE_FULL;
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    m.apply(CampaignCommand::Recruit { region: RegionId(13), unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    let item = m.world.regions[&RegionId(13)].recruitment_queue[0].clone();
    assert_eq!(item.target, Some(general));
    let r = m.commander_recruitment(A, general);
    assert_eq!(r.queue, vec![(RegionId(13), item.id)]);
    assert!(r.options.iter().all(|o| o.flags & ENTRY_QUEUE_FULL == 0));
    // Ten for him, spread over both regions (neither queue is full on its own).
    for i in 0..9 {
        let region = if i % 2 == 0 { RegionId(10) } else { RegionId(13) };
        m.apply(CampaignCommand::Recruit { region, unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    }
    let r = m.commander_recruitment(A, general);
    assert_eq!(r.queue.len(), 10);
    assert!(r.options.iter().all(|o| o.flags & ENTRY_QUEUE_FULL != 0 && !o.available()), "{r:?}");
}

/// When the commander an item was queued through dies, the item stays queued and unrefunded and only loses
/// its target (item listener `0x00B57F40`, notified from the character destructor `0x0099D2D0`).
#[test]
fn a_dead_commanders_queued_items_stay_and_lose_their_target() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    m.apply(CampaignCommand::Recruit { region: RegionId(13), unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    let treasury = m.world.factions[&A].treasury;
    let own = m.world.regions[&RegionId(10)].recruitment_queue.clone();
    let mut item = m.world.regions[&RegionId(13)].recruitment_queue[0].clone();
    m.character_dies(general);
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue, own, "an untargeted item is untouched");
    item.target = None;
    assert_eq!(m.world.regions[&RegionId(13)].recruitment_queue, vec![item]);
    assert_eq!(m.world.factions[&A].treasury, treasury, "no refund");
}

/// ORIGINAL BUG (`0x00B0F2B0` refuses a path whose start is its goal): a general inside a settlement
/// recruits its units with no march; the exe flagged them as having no path.
#[test]
fn a_garrisoned_general_recruits_his_settlements_units_without_a_march() {
    let mut m = commander_recruitment_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(10));
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.travel_turns, o.flags), (RegionId(10), 0.0, 0), "{o:?}");
}

/// The same ORIGINAL BUG in the other order: the untrainable source is the first and the nearer one.
/// The exe cuts the next source's search at the first one's path cost and so refuses the farther,
/// trainable region; ours searches without the cut while the best cannot train.
#[test]
fn a_nearer_first_source_without_recruitment_points_loses_to_a_farther_one() {
    let mut m = commander_recruitment_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(0, 0);
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_no_points".into(), BuildingRules { chain: "test_chain_2".into(), units_allowed: vec!["test_recruit".into()], ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building = Some(BuildingRef { level_key: "test_no_points".into(), health: 100 });
    assert_eq!(m.recruitment_points(RegionId(10), false), 0);
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.training_turns), (RegionId(13), 2), "{o:?}");
}

/// `units` #9 divides the march (`0x00B41F60`, unguarded in the exe); a mod's 0 is read as 1, not a
/// division by zero.
#[test]
fn a_march_speed_of_zero_is_read_as_one() {
    let mut m = commander_recruitment_model();
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_recruit").unwrap().travel_speed = 0;
    m.rules = Arc::new(rules);
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert!((o.travel_turns - 7.0).abs() < 1e-6 && o.travel_turns_rounded() == 7, "{o:?}");
}

#[test]
fn model_equality_ignores_the_unsaved_negotiation_slot() {
    let a = test_model();
    let mut b = a.clone();
    b.negotiations.begun = 3;
    b.negotiations.ended = 1;
    assert_eq!(a, b, "the negotiation slot is neither saved nor hashed");
    assert_eq!(a.state_hash(), b.state_hash());
    b.deal_inflation.first += 1;
    assert_ne!(a, b, "saved state still counts");
}
