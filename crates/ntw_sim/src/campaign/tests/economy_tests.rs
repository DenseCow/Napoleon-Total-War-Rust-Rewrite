//! Campaign model tests: economy.

use super::*;
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
    // The round start books upkeep, taxes and trade into A's first record (other income from creation).
    economy::refresh_running_record_round_start(&mut m, A);
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

/// The next running record keeps the tracker categories 8..11 and 18..24, clears the one-off groups
/// and category 6, and takes taxes and trade from the paid one (`0x00B955C0` then `0x00BBD7C0(0..2)`).
#[test]
fn the_next_running_record_carries_the_trackers() {
    let paid: super::world::EconomyRecord = std::array::from_fn(|i| 100 + i as i32);
    let next = economy::next_running_record(&paid);
    for i in 0..25 {
        let expected = match i {
            5 | 7 | 8..=11 | 18..=24 => paid[i],
            _ => 0,
        };
        assert_eq!(next[i], expected, "category {i}");
    }
}

/// The round end pays the running record (`0x00BABE30` → `0x00BBE970` / `0x00BBE910`) and starts the
/// next from it: categories the model does not keep (income 8, expenses 21 here) stay in the paid record
/// and carry on; one-off spending (0..4, 12..17) stays in the paid record only. Category 11 is the
/// record's, not recomputed.
#[test]
fn the_round_end_keeps_unmodelled_categories() {
    let mut m = test_model();
    let mut running = [0; 25];
    (running[0], running[8], running[11], running[13], running[21]) = (500, 40, 7, 60, 77);
    m.world.economy_history.insert(A, vec![running]);
    economy::refresh_running_record_round_start(&mut m, A);
    let income = economy::faction_income(&m, A);
    let treasury = m.world.factions[&A].treasury;
    economy::settle_round(&mut m, A);
    let history = &m.world.economy_history[&A];
    assert_eq!(history.len(), 2);
    let (paid, next) = (&history[0], &history[1]);
    assert_eq!((paid[0], paid[8], paid[11], paid[13], paid[21]), (500, 40, 7, 60, 77));
    assert_eq!((paid[5], paid[7], paid[9]), (income.taxes, income.trade, income.raiding));
    assert_eq!((next[0], next[8], next[11], next[13], next[21]), (0, 40, 7, 0, 77));
    let revenue = income.taxes + income.trade + income.raiding + 40 + 7;
    assert_eq!(m.world.last_income(A), revenue);
    assert_eq!(m.world.last_expenses(A), income.upkeep + 77);
    assert_eq!(m.world.factions[&A].treasury, treasury + revenue - income.upkeep - 77);
}

/// The round end pays the record's taxes as last refreshed, not the current ones (`0x00BABE30` pays the
/// running record; `0x00BBD7C0(0)` writes taxes only at its callers).
#[test]
fn the_round_end_pays_the_recorded_taxes() {
    let mut m = test_model();
    economy::refresh_running_record_round_start(&mut m, A);
    let recorded = *m.world.last_economy_record(A).unwrap();
    // A region gets richer after the refresh: its taxes count only from the next refresh.
    m.world.regions.get_mut(&RegionId(10)).unwrap().gdp += 5000;
    assert_ne!(economy::faction_income(&m, A).taxes, recorded[5]);
    let treasury = m.world.factions[&A].treasury;
    economy::settle_round(&mut m, A);
    let paid = m.world.economy_history[&A][0];
    assert_eq!(paid[5], recorded[5]);
    let (revenue, expenses): (i32, i32) = (paid[5..12].iter().sum(), paid[18..25].iter().sum());
    assert_eq!(m.world.factions[&A].treasury, treasury + revenue - expenses);
}

/// The running record's taxes follow a tax-level change (`0x00BC5380` / `0x00BC5440` →
/// `0x00BBD7C0(0)`) and a region's change of owner, for both owners (`0x00A6512D` / `0x00A6513E`).
#[test]
fn the_running_taxes_follow_tax_levels_and_owners() {
    let mut m = test_model();
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Lower, level: "tax_high".into() }).unwrap();
    assert_eq!(m.world.last_economy_record(A).unwrap()[5], economy::faction_income(&m, A).taxes);
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Upper, level: "tax_high".into() }).unwrap();
    assert_eq!(m.world.last_economy_record(A).unwrap()[5], 2400);
    m.world.economy_history.insert(B, vec![[0; 25]]);
    assert!(m.transfer_region(RegionId(12), B));
    assert_eq!(m.world.last_economy_record(A).unwrap()[5], economy::faction_income(&m, A).taxes);
    assert_eq!(m.world.last_economy_record(B).unwrap()[5], economy::faction_income(&m, B).taxes);
    assert_ne!(m.world.last_economy_record(A).unwrap()[5], 2400);
}

/// The round start writes taxes, category 6 and trade of each listed faction in the game into its
/// running record and books the upkeep trackers' change, new − booked, into 19 / 20 (`0x008F1F60`
/// step 1d, `0x0088F950` / `0x0088F990`).
#[test]
fn the_round_start_refreshes_the_running_record() {
    let mut m = test_model();
    m.world.economy_history.insert(A, vec![[9; 25]]);
    economy::refresh_running_record_round_start(&mut m, A);
    let income = economy::faction_income(&m, A);
    let r = *m.world.last_economy_record(A).unwrap();
    assert_eq!((r[5], r[6], r[7]), (income.taxes, 0, income.trade));
    // Nothing was booked before: the whole upkeep is added to the 9 already there.
    assert_eq!(r[19] + r[20], income.upkeep + 18);
    // A second refresh with the same upkeep books nothing more.
    economy::refresh_running_record_round_start(&mut m, A);
    let r2 = m.world.last_economy_record(A).unwrap();
    assert_eq!(r2[19] + r2[20], income.upkeep + 18);
    // The other categories are left as they were.
    assert_eq!((r[0], r[8], r[11], r[21]), (9, 9, 9, 9));
    // A faction without a history gets one record.
    economy::refresh_running_record_round_start(&mut m, B);
    assert_eq!(m.world.economy_history[&B].len(), 1);
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
    economy::refresh_running_record_round_start(&mut m, B);
    m.world.factions.get_mut(&B).unwrap().treasury = -150;
    // B's record: income 100, upkeep 20: -150 + 100 < 20.
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
        (CampaignCommand::MakePeace { a: A, b: B, suppress_messages: false }, CommandError::NotAtWar(A, B)),
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
    m.apply(CampaignCommand::MakePeace { a: B, b: A, suppress_messages: false }).unwrap();
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
    // C holds nothing but stays in the game (faction +0x824 clear), so it plays its turns.
    m.world.faction_details.entry(C).or_default().out_of_game = Some(false);
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

/// Each trade update ends by writing every listed faction's running trade (`0x00BBD7C0(2)` at
/// `0x00BCE78E`).
#[test]
fn the_trade_update_refreshes_the_running_trade() {
    let mut m = test_model();
    m.world.economy_history.insert(A, vec![[999; 25]]);
    m.update_commodity_prices();
    let r = m.world.last_economy_record(A).unwrap();
    assert_eq!(r[7], economy::trade_routes_value(&m, A));
    assert_eq!((r[5], r[6]), (999, 999));
}
