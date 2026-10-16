//! Campaign model tests: hiring.

use super::*;

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

/// Peninsular loot resentment (`0x00B541E0` → `0x00B544F0`): looting a region whose rebel key names a homeland faction
/// adds −50 to that faction's `abandoned_ally_in_war` factor towards the looter, unless the two are at war, the looter
/// is exempt or is the homeland itself; occupying never does.
#[test]
fn looting_a_homeland_region_angers_its_faction() {
    use super::capture::CaptureChoice;
    use super::features::LootResentment;
    use super::treaties::slot;
    let run = |homeland: &str, exempt: Option<&str>, c_at_war: bool, choice: CaptureChoice| {
        let mut m = capture_model(vec![A]);
        let mut rules = (*m.rules).clone();
        rules.features.loot_resentment = Some(LootResentment {
            homelands: vec![("other_rebels".into(), "test_faction_b".into()), ("homeland_rebels".into(), homeland.into())],
            exempt_looter: exempt.map(str::to_owned),
            factor: "abandoned_ally_in_war".into(),
            value: -50,
        });
        m.rules = Arc::new(rules);
        m.world.region_rebel_factions.insert(RegionId(11), "homeland_rebels".into());
        if c_at_war {
            m.world.set_stance(C, A, Stance::War).unwrap();
        }
        m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
        m.apply(CampaignCommand::ChooseCapture { choice }).unwrap();
        m.world.relationships.get(&(C, A)).map(|r| r.attitudes[slot("abandoned_ally_in_war")])
    };
    let f = run("test_faction_c", Some("test_faction_b"), false, CaptureChoice::Loot).expect("C's record towards A");
    // The event's drift 2 and limit 0 (`abandoned_ally_in_war`, event 0).
    assert_eq!((f.value, f.drift, f.limit, f.limited), (-50, 2, 0, true));
    for (homeland, exempt, war, choice, why) in [
        ("test_faction_c", Some("test_faction_a"), false, CaptureChoice::Loot, "an exempt looter"),
        ("test_faction_c", None, true, CaptureChoice::Loot, "at war"),
        ("test_faction_c", None, false, CaptureChoice::Occupy, "an occupation"),
        ("test_faction_a", None, false, CaptureChoice::Loot, "the looter's own homeland"),
    ] {
        let v = run(homeland, exempt, war, choice).map_or(0, |f| f.value);
        assert_eq!(v, 0, "{why} angers no one");
    }
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
    m.world.faction_details.get_mut(&A).unwrap().bonus_base = vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new(), ..Default::default() }];
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
        vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new(), ..Default::default() }, SavedBonus { kind: 1, bonus: 65, value: 1.0, qualifier: String::new(), ..Default::default() }];
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
    m.world.faction_details.get_mut(&A).unwrap().bonus_base = vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new(), ..Default::default() }];
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

/// `0x009D1CD0`: the gate tests neither the pool, the purse nor the turn; only the rebels (and a
/// force without a commander, whose byte +0x1B7 the exe reads) are refused.
#[test]
fn the_interface_gate_for_hiring_a_commander_refuses_only_the_rebels() {
    let mut m = pool_model();
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed + 100);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    m.turn.humans = vec![A];
    m.start_campaign();
    assert!(m.can_recruit_commander(ForceId(1002)), "an empty pool still opens the panel");
    assert!(m.can_recruit_commander(ForceId(1003)));
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    m.turn.current = Some(B);
    assert!(m.can_recruit_commander(ForceId(1002)), "neither debt nor another faction's turn closes it");
    assert!(!m.can_recruit_commander(ForceId(9999)), "no such force");
    m.world.forces.get_mut(&ForceId(1002)).unwrap().commander = None;
    assert!(!m.can_recruit_commander(ForceId(1002)), "no commander");
    m.world.factions.get_mut(&A).unwrap().key = String::new();
    assert!(!m.can_recruit_commander(ForceId(1003)), "the rebels");
}

/// `0x009D52A0`: the faction's characters of the pool's type less the pool's own candidates.
#[test]
fn commanders_in_service_leave_out_the_candidates() {
    use super::pool::PoolKind;
    let mut m = pool_model();
    let generals = |m: &CampaignModel| m.world.characters.values().filter(|c| c.faction == A && c.kind == CharacterKind::General).count() as u32;
    let before = generals(&m);
    assert_eq!(m.commanders_in_service(A, PoolKind::General), before);
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    let c = m.pool_tick(A)[0];
    assert_eq!(generals(&m), before + 1, "the candidate is a General of the faction");
    assert_eq!(m.commanders_in_service(A, PoolKind::General), before, "but not in service");
    m.world.faction_details.get_mut(&A).unwrap().general_pool.0.retain(|&x| x != c);
    assert_eq!(m.commanders_in_service(A, PoolKind::General), before + 1);
    assert_eq!(m.commanders_in_service(A, PoolKind::Admiral), 0);
}

/// `0x009D1E00`: `agents` #11 plus the `cap` bonus (type 0, id 2, rounded half to even), held to
/// `agents` #10 when that is not 0; a negative sum is 0 (ORIGINAL BUG: read unsigned there).
#[test]
fn the_most_commanders_a_faction_may_have_add_the_cap_bonus_up_to_the_ceiling() {
    use super::effects::SavedBonus;
    use super::pool::PoolKind;
    let mut m = pool_model();
    assert_eq!(m.commanders_max(A, PoolKind::General), 4);
    assert_eq!(m.commanders_max(A, PoolKind::Admiral), 3);
    let cap = |v: f32, agent: &str| SavedBonus { kind: 0, bonus: 2, value: v, qualifier: agent.into(), ..Default::default() };
    m.world.faction_details.entry(A).or_default().bonus_base = vec![cap(2.0, "General")];
    assert_eq!(m.commanders_max(A, PoolKind::General), 6, "the majors' agent_cap_general +2");
    assert_eq!(m.commanders_max(A, PoolKind::Admiral), 3, "qualified by the agent key");
    assert_eq!(m.commanders_max(B, PoolKind::General), 4, "per faction");
    m.world.faction_details.entry(A).or_default().bonus_base = vec![cap(2.5, "General")];
    assert_eq!(m.commanders_max(A, PoolKind::General), 6, "2.5 rounds half to even");
    m.world.faction_details.entry(A).or_default().bonus_base = vec![cap(-10.0, "General")];
    assert_eq!(m.commanders_max(A, PoolKind::General), 0, "a negative sum lifts no limit");
    m.world.faction_details.entry(A).or_default().bonus_base = vec![cap(9.0, "General")];
    Arc::make_mut(&mut m.rules).agent_caps.insert("General".into(), (4, 12));
    assert_eq!(m.commanders_max(A, PoolKind::General), 12, "held to the ceiling");
    Arc::make_mut(&mut m.rules).agent_caps.clear();
    assert_eq!(m.commanders_max(A, PoolKind::General), 9, "a type without a row has base 0");
}

/// `0x00A1BB20`: a row's flags, bit 0 at the maximum, bit 1 too dear, bit 2 no room; the row is
/// recruitable only when all are clear. The room (`0x009FB660` / `0x009FB640`): a free unit slot
/// for a General, a ship without an admiral for an admiral.
#[test]
fn a_pool_row_flags_the_maximum_the_purse_and_the_room() {
    use super::pool::{PoolKind, ROW_AT_MAXIMUM, ROW_NO_ROOM, ROW_TOO_DEAR};
    let mut m = pool_model();
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed);
    let made = m.pool_tick(A);
    let g = *made.iter().find(|&&c| m.world.characters[&c].kind == CharacterKind::General).unwrap();
    let a = *made.iter().find(|&&c| m.world.characters[&c].kind == CharacterKind::Admiral).unwrap();
    assert_eq!(m.commander_row_flags(g, ForceId(1002)), Some(0));
    assert_eq!(m.commander_row_flags(a, ForceId(1003)), Some(0));
    assert_eq!(m.commander_row_flags(a, ForceId(1002)), None, "an army lists the General pool");
    assert_eq!(m.commander_row_flags(CharacterId(107), ForceId(1002)), None, "not a candidate");
    // Bit 0: as many in service as allowed.
    Arc::make_mut(&mut m.rules).agent_caps.insert("General".into(), (0, 0));
    assert_eq!(m.commander_row_flags(g, ForceId(1002)), Some(ROW_AT_MAXIMUM));
    // The panel counts the pool once and passes the counts in (`commander_row_flags_with`): same flags.
    let (in_service, max) = (m.commanders_in_service(A, PoolKind::General), m.commanders_max(A, PoolKind::General));
    assert_eq!(max, 0);
    assert_eq!(m.commander_row_flags_with(g, ForceId(1002), in_service, max), Some(ROW_AT_MAXIMUM));
    assert_eq!(m.commander_row_flags_with(g, ForceId(1002), 0, 1), Some(0));
    Arc::make_mut(&mut m.rules).agent_caps.insert("General".into(), (4, 0));
    // Bit 1: the treasury cannot pay.
    m.world.factions.get_mut(&A).unwrap().treasury = 0;
    assert_eq!(m.commander_row_flags(g, ForceId(1002)), Some(ROW_TOO_DEAR));
    m.world.factions.get_mut(&A).unwrap().treasury = 100_000;
    // Bit 2: a full army; a fleet whose every ship has an admiral.
    m.force_caps.army = 1;
    assert!(!m.has_commander_room(ForceId(1002), PoolKind::General));
    assert_eq!(m.commander_row_flags(g, ForceId(1002)), Some(ROW_NO_ROOM));
    assert!(!m.has_commander_room(ForceId(1002), PoolKind::Admiral), "an army has no ship");
    assert!(m.has_commander_room(ForceId(1003), PoolKind::Admiral), "the captain's ship");
    m.world.characters.get_mut(&CharacterId(108)).unwrap().kind = CharacterKind::Admiral;
    assert_eq!(m.commander_row_flags(a, ForceId(1003)), Some(ROW_NO_ROOM));
}

/// `0x00A278B0`: the admiral boards the fleet's first ship without an admiral, replacing its
/// character; then the command is picked again (`0x008B8E50`), so an admiral aboard may keep it.
/// A fleet of admirals only is refused.
#[test]
fn a_hired_admiral_boards_the_first_ship_without_an_admiral() {
    use super::pool::PoolKind;
    let mut m = pool_model();
    m.world.characters.insert(CharacterId(109), character(109, A, CharacterKind::Admiral, 25));
    let mut flagship = unit(9);
    flagship.character = Some(CharacterId(109));
    let fleet = m.world.forces.get_mut(&ForceId(1003)).unwrap();
    fleet.units.insert(0, flagship);
    fleet.commander = Some(CharacterId(109));
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed + 100);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed);
    let c = m.pool_tick(A)[0];
    m.turn.humans = vec![A];
    m.start_campaign();
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    let f = &m.world.forces[&ForceId(1003)];
    assert_eq!(f.units[0].character, Some(CharacterId(109)), "the flagship keeps its admiral");
    assert_eq!(f.units[1].character, Some(c), "the new admiral replaces the captain");
    assert!(!m.world.characters.contains_key(&CharacterId(108)), "the captain is destroyed");
    let lead = m.commander_unit(ForceId(1003)).and_then(|i| f.units[i].character);
    assert!(lead == Some(CharacterId(109)) || lead == Some(c));
    assert_eq!(f.commander, lead, "the command is picked again among the admirals");
    assert!(!m.has_commander_room(ForceId(1003), PoolKind::Admiral));
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), m.calendar.turns_elapsed);
    let d = m.pool_tick(A)[0];
    assert!(m.apply(CampaignCommand::HireAdmiral { character: d, fleet: ForceId(1003) }).is_err(), "every ship has an admiral");
}
