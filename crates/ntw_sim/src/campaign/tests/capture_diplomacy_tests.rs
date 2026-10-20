//! Campaign model tests: capture diplomacy.

use super::*;

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
    assert!(r.class_bases.iter().all(|c| c.resistance == -10), "{:?}", r.class_bases);
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

/// `0x00B14930`: an option's public-order previews are the net order (`0x008E2AB0`) of the classes the
/// capturer's government names: +0x18 the upper class (`0x00A61400`), +0x1C the lower (`0x00A610C0`), and 0 for
/// a class the region does not have (not the first class, as `0x00A61320` / `0x00A60FE0` fall back to).
#[test]
fn capture_previews_are_the_governments_upper_and_lower_classes() {
    use super::capture::CaptureChoice;
    let setup = |classes: &[(&str, i32)]| {
        let mut m = capture_model(vec![A]);
        m.world.factions.get_mut(&A).unwrap().government_key = "gov_constitutional".into();
        Arc::make_mut(&mut m.rules).government_classes.insert("gov_constitutional".into(), ("upper".into(), "middle".into()));
        m.world.regions.get_mut(&RegionId(11)).unwrap().class_bases = classes.iter().map(|&(c, res)| PopulationClassState { resistance: res, ..PopulationClassState::new(c) }).collect();
        m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
        m
    };
    let total = |m: &CampaignModel, class: &str| {
        economy::public_order_factors(m, RegionId(11)).iter().find(|f| f.class == class).map(|f| f.total() as f32)
    };
    // A constitutional monarchy governs `upper` and `middle`: the region's `lower` class feeds neither preview.
    let mut m = setup(&[("lower", 7), ("middle", -3), ("upper", 5)]);
    let p = m.pending_capture.clone().unwrap();
    let (lower, upper) = p.occupy.public_order_after.unwrap();
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert_eq!((Some(lower), Some(upper)), (total(&m, "middle"), total(&m, "upper")));
    assert_ne!(Some(lower), total(&m, "lower"), "the region's own `lower` class is not the government's lower class");
    // The loot preview counts its public-order reduction on both classes.
    let mut m = setup(&[("lower", 7), ("middle", -3), ("upper", 5)]);
    let p = m.pending_capture.clone().unwrap();
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
    assert_eq!(p.loot.public_order_after, Some((total(&m, "middle").unwrap(), total(&m, "upper").unwrap())));
    // No `middle` class in the region: the lower preview is 0, not the first class's order.
    let mut m = setup(&[("upper", 5), ("lower", 7)]);
    let (lower, upper) = m.pending_capture.clone().unwrap().occupy.public_order_after.unwrap();
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert_eq!(lower, 0.0);
    assert_ne!(upper, 0.0, "the first class (upper) has an order of its own");
    assert_eq!(Some(upper), total(&m, "upper"));
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
    let mut no_capital = m.clone();
    let pos_before = m.world.characters[&CharacterId(100)].position;
    set_details(&mut m, A, Some(10), false);
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, C);
    // The liberation expels the capturer's army from the region (0x008D9DC0 at 0x00B508C5, before
    // the owner change), so a capturer without a capital, which the owner change's own pass
    // (0x00A64AC0, factions with a capital only) skips, is moved out of the settlement too (it stays in the region on the map: the owner is still
    // the liberator at expel time, so it may stay).
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(11)].garrison, None);
    assert_eq!(no_capital.world.capital(A), None);
    no_capital.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(no_capital.world.regions[&RegionId(11)].owner, C);
    assert_eq!(no_capital.world.regions[&RegionId(11)].garrison, None);
    assert_eq!(no_capital.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(no_capital.world.characters[&CharacterId(100)].position, pos_before);
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
fn the_scripts_liberation_switch_offers_no_liberation() {
    use super::capture::CaptureChoice;
    // Episodic restrictions +0x124 (set_liberation_options_disabled): the capture options
    // 0x00B14930 offer no liberation while it is set (tests 0x00B15134 / 0x00B15214), so a
    // liberate choice loots, as with no target.
    let mut m = capture_model(vec![A]);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
    assert_eq!(m.liberation_target(RegionId(11), A), Some(C));
    m.world.liberation_options_disabled = true;
    assert_eq!(m.liberation_target(RegionId(11), A), None);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(m.pending_capture.as_ref().unwrap().liberate, None);
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CaptureResolved { choice: CaptureChoice::Loot, .. })));
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
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
    m.apply(CampaignCommand::MakePeace { a: A, b: B, suppress_messages: false }).unwrap();
    let r = &m.world.relationships[&(A, B)];
    assert_eq!(r.attitudes[super::treaties::slot("war")].value, -20);
    assert_eq!(r.friendship_turns, 10);
    // Declaring war again during the friendship turns is backstabbing: the count (preset 1, 0x00AE8DD0) grows by 3 (10 turns).
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(m.world.treaty_breaks[&A], 4);
}

/// The diplomacy manager counters a war reads and bumps: the treaty-break count (+0x1C, preset 1 by
/// `0x00AE8DD0`, so the first backstab is penalised; `0x00B0E420`) and the access-abuse count (+0x18,
/// `0x00B0DA90`, the `count` of the `abused_military_access` add, then +1).
#[test]
fn war_reads_and_bumps_the_diplomacy_manager_counters() {
    use super::treaties::slot;
    let mut m = test_model();
    assert_eq!(m.world.treaty_breaks_of(A), super::TREATY_BREAKS_START);
    assert_eq!(super::TREATY_BREAKS_START, 1);
    let war = |m: &mut CampaignModel, friendship: u32| {
        m.relationship_mut(A, B).friendship_turns = friendship;
        m.relationship_mut(B, A).military_access_turns = 10;
        m.relationship_mut(B, A).attitudes[slot("abused_military_access")].value = 0;
        m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    };
    let peace_treaty = |m: &CampaignModel| {
        let f = m.world.relationships[&(B, A)].attitudes[slot("peace_treaty")];
        (f.value, f.drift, f.limit)
    };
    // No friendship: no backstab; B had given A access: −30 + 0 × (−10), clamped to ≤ 0.
    war(&mut m, 0);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[slot("abused_military_access")].value, -30);
    assert_eq!(m.world.access_abuses[&A], 1);
    assert!(!m.world.treaty_breaks.contains_key(&A));
    m.apply(CampaignCommand::MakePeace { a: A, b: B, suppress_messages: false }).unwrap();
    // First backstab, 4 friendship turns: n = 1 → value −3·1·4, limit −5·1; then n + 1.
    war(&mut m, 4);
    assert_eq!(peace_treaty(&m), (-12, 1, -5));
    assert_eq!(m.world.treaty_breaks[&A], 2);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[slot("abused_military_access")].value, -40, "second abuse: −30 + 1 × (−10)");
    assert_eq!(m.world.access_abuses[&A], 2);
    m.apply(CampaignCommand::MakePeace { a: A, b: B, suppress_messages: false }).unwrap();
    // A backstab at 10 friendship turns: n = 2 → value −60, limit −10; then n + 3.
    war(&mut m, 10);
    assert_eq!(peace_treaty(&m), (-60, 1, -10));
    assert_eq!(m.world.treaty_breaks[&A], 5);
    assert_eq!(m.world.access_abuses[&A], 3);
}

/// A backstab's other-faction penalty (`0x00B0E540`) goes to the factions of the declarer's own
/// subculture, not the victim's: `0x00B0E420` reads the declarer's faction record +0x10 at
/// `0x00B0E446` and compares every other faction's against it.
#[test]
fn a_backstab_angers_the_declarers_subculture_not_the_victims() {
    use super::treaties::slot;
    let backstab = |subs: [&str; 3]| {
        let mut m = test_model();
        let mut rules = (*m.rules).clone();
        for (k, s) in ["test_faction_a", "test_faction_b", "test_faction_c"].into_iter().zip(subs) {
            rules.faction_subcultures.insert(k.into(), s.into());
        }
        m.rules = Arc::new(rules);
        m.relationship_mut(A, B).friendship_turns = 4;
        m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
        let f = m.world.relationships.get(&(C, A)).map(|r| r.attitudes[slot("peace_treaty")]).unwrap_or_default();
        (f.value, f.drift, f.limit)
    };
    // C shares the declarer A's subculture: n = 1 → value −5·1·4, limit −25·1.
    assert_eq!(backstab(["sc_a", "sc_b", "sc_a"]), (-20, 1, -25));
    // C shares only the victim B's subculture: untouched.
    assert_eq!(backstab(["sc_a", "sc_b", "sc_b"]), (0, 0, 0));
}

/// A deal's embargo item (`0x00C18800` → `0x00B28DB0`): A embargoes B at C's request. A's embargo
/// turns towards B are 10, and B resents C, the side that asked for it; B's attitude to A is untouched.
#[test]
fn a_deal_embargo_makes_the_embargoed_faction_resent_the_requester() {
    use super::treaties::{slot, DiplomaticAction as D};
    let mut m = test_model();
    m.apply(CampaignCommand::Diplomacy { a: A, b: C, action: D::Embargo(B) }).unwrap();
    assert_eq!(m.world.relationships[&(A, B)].trade_embargo_turns, 10);
    let e = m.world.relationships[&(B, C)].attitudes[slot("trade_embargoed")];
    assert_eq!((e.value, e.drift, e.limit), (-40, -2, 0));
    let towards_embargoer = m.world.relationships.get(&(B, A)).map_or(0, |r| r.attitudes[slot("trade_embargoed")].value);
    assert_eq!(towards_embargoer, 0);
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
    act(&mut m, A, C, D::Embargo(B));
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

/// The `alliance_broken` message (type 11, `0x009CA7B0`): an alliance break between two major powers
/// goes to every human major power in faction order, titled "<ally> - <breaker>" (`0x00B13840`); none
/// when either side is minor. A cancelled military access sends it to the faction losing the access,
/// human or not (`0x00B67B20`).
#[test]
fn alliance_broken_messages_follow_the_exe() {
    use super::treaties::DiplomaticAction as D;
    use super::CampaignMessage;
    let mut m = test_model();
    for f in [A, B, C] {
        m.world.faction_details.entry(f).or_default().major = Some(true);
    }
    m.turn.humans = vec![C, A];
    let act = |m: &mut CampaignModel, a, b, action| m.apply(CampaignCommand::Diplomacy { a, b, action }).unwrap();
    let broken = |recipient, first, second| CampaignEvent::Message { recipient, message: CampaignMessage::AllianceBroken { first, second } };
    act(&mut m, A, B, D::Alliance);
    assert_eq!(
        act(&mut m, A, B, D::BreakAlliance),
        vec![broken(A, B, A), broken(C, B, A), CampaignEvent::StanceChanged { a: A, b: B, stance: Stance::Neutral }],
        "the human majors in faction order, B (AI) gets none"
    );
    // The order is the world's faction list (`FACTION_ARRAY` order, model +0xF5C → +0x30), not the ids.
    m.world.turn_order = vec![C, B, A];
    act(&mut m, A, B, D::Alliance);
    assert_eq!(
        act(&mut m, A, B, D::BreakAlliance),
        vec![broken(C, B, A), broken(A, B, A), CampaignEvent::StanceChanged { a: A, b: B, stance: Stance::Neutral }],
        "the human majors in faction-list order"
    );
    // A minor ally: no message.
    m.world.faction_details.entry(B).or_default().major = Some(false);
    act(&mut m, A, B, D::Alliance);
    assert_eq!(act(&mut m, A, B, D::BreakAlliance), vec![CampaignEvent::StanceChanged { a: A, b: B, stance: Stance::Neutral }]);
    // A human minor gets none either.
    m.world.faction_details.entry(B).or_default().major = Some(true);
    m.world.faction_details.entry(C).or_default().major = Some(false);
    act(&mut m, B, A, D::Alliance);
    assert_eq!(
        act(&mut m, B, A, D::BreakAlliance),
        vec![broken(A, A, B), CampaignEvent::StanceChanged { a: B, b: A, stance: Stance::Neutral }]
    );
    // Cancelled access: B (an AI) is told.
    act(&mut m, A, B, D::GrantMilitaryAccess(10));
    assert_eq!(act(&mut m, A, B, D::CancelMilitaryAccess), vec![broken(B, B, A)]);
    assert_eq!(CampaignMessage::AllianceBroken { first: A, second: B }.type_name(), "alliance_broken");
}

/// Becoming a protectorate (`0x00B105C0`) settles the new protectorate's own ties first (`0x00B104E0`): a patron
/// it had loses it with no alliance grievance (`0x00B64B10`), a protectorate it had passes to the new patron; then
/// its remaining alliances break (`0x00B13840`).
#[test]
fn a_new_protectorate_settles_its_old_ties() {
    use super::treaties::slot;
    let broken = |m: &CampaignModel, a: FactionId, b: FactionId| m.world.relationships.get(&(a, b)).map_or(0, |r| r.attitudes[slot("alliance_broken")].value);
    // C was B's protectorate: the tie ends (both neutral, tribute and income cleared, no grievance).
    let mut m = test_model();
    m.make_protectorate(C, B).unwrap();
    m.relationship_mut(C, B).protectorate_tribute = 40;
    m.relationship_mut(B, C).protectorate_income = 40;
    m.make_protectorate(C, A).unwrap();
    assert_eq!((m.world.stance(C, A), m.world.stance(C, B), m.world.stance(B, C)), (Stance::Protectorate, Stance::Neutral, Stance::Neutral));
    assert_eq!((m.world.relationships[&(C, B)].protectorate_tribute, m.world.relationships[&(B, C)].protectorate_income), (0, 0));
    assert_eq!((broken(&m, B, C), broken(&m, C, B)), (0, 0), "no alliance grievance");
    // C was B's patron: B passes to A as its protectorate, and B's tie to C ends.
    let mut m = test_model();
    m.make_protectorate(B, C).unwrap();
    m.make_protectorate(C, A).unwrap();
    assert_eq!((m.world.stance(C, A), m.world.stance(B, A), m.world.stance(B, C)), (Stance::Protectorate, Stance::Protectorate, Stance::Neutral));
    assert_eq!((broken(&m, B, C), broken(&m, C, B)), (0, 0));
    // C was B's ally: the protectorate's alliance breaks (B holds the grievance), the patron's would not.
    let mut m = test_model();
    m.form_alliance(C, B).unwrap();
    m.make_protectorate(C, A).unwrap();
    assert_eq!((m.world.stance(C, A), m.world.stance(C, B)), (Stance::Protectorate, Stance::Neutral));
    assert!(broken(&m, B, C) < 0, "B holds the broken alliance against C");
    let mut m = test_model();
    m.form_alliance(A, B).unwrap();
    m.make_protectorate(C, A).unwrap();
    assert_eq!(m.world.stance(A, B), Stance::Allied, "the patron's alliances stay");
}

/// `CCQ_MAKE_PROTECTORATE` (`0x009367A0`): refused for an unknown or the same faction, at war, or when either
/// side already has a patron (`0x008CEEA0`); otherwise the second faction becomes the first's protectorate
/// (`0x00B105C0`). No turn test.
#[test]
fn the_make_protectorate_command_follows_the_exe_handler() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(C, vec![C]);
    assert_eq!(m.apply(CampaignCommand::MakeProtectorate { patron: A, protectorate: A }), Err(CommandError::SameFaction(A)));
    assert_eq!(m.apply(CampaignCommand::MakeProtectorate { patron: A, protectorate: FactionId(99) }), Err(CommandError::UnknownFaction(FactionId(99))));
    m.apply(CampaignCommand::DeclareWar { a: C, b: B }).unwrap();
    assert_eq!(m.apply(CampaignCommand::MakeProtectorate { patron: B, protectorate: C }), Err(CommandError::AlreadyAtWar(B, C)));
    m.apply(CampaignCommand::MakeProtectorate { patron: A, protectorate: B }).unwrap();
    assert_eq!((m.world.stance(B, A), m.world.stance(A, B)), (Stance::Protectorate, Stance::Patron));
    assert!(m.has_patron(B) && !m.has_patron(A));
    // B has a patron now: it may take part in no other.
    m.apply(CampaignCommand::MakePeace { a: C, b: B, suppress_messages: true }).unwrap();
    assert_eq!(m.apply(CampaignCommand::MakeProtectorate { patron: C, protectorate: B }), Err(CommandError::HasPatron(B)));
    assert_eq!(m.apply(CampaignCommand::MakeProtectorate { patron: B, protectorate: C }), Err(CommandError::HasPatron(B)));
    // The patron itself may take a second protectorate.
    m.apply(CampaignCommand::MakeProtectorate { patron: A, protectorate: C }).unwrap();
    assert_eq!(m.world.stance(C, A), Stance::Protectorate);
}

/// `CCQ_MOVE_ARMIES_OUT_OF_REGIONS` (`0x00936AB0`): the second faction's forces leave the first's regions,
/// then the first's the second's, whatever their stance; a force that may stay still leaves the settlement.
#[test]
fn moving_armies_out_empties_both_sides_settlements() {
    let mut m = test_model();
    let garrison = |m: &mut CampaignModel, force: u32, ch: i32, region: u32| {
        m.world.regions.get_mut(&RegionId(region)).unwrap().garrison = Some(ForceId(force));
        m.world.characters.get_mut(&CharacterId(ch)).unwrap().garrisoned_in = Some(RegionId(region));
    };
    garrison(&mut m, 1000, 100, 11);
    garrison(&mut m, 1001, 101, 10);
    assert_eq!(m.apply(CampaignCommand::MoveArmiesOutOfRegions { a: A, b: FactionId(99) }), Err(CommandError::UnknownFaction(FactionId(99))));
    assert_eq!(m.world.regions[&RegionId(10)].garrison, Some(ForceId(1001)), "nothing moved on a refusal");
    m.apply(CampaignCommand::MoveArmiesOutOfRegions { a: A, b: B }).unwrap();
    assert_eq!((m.world.regions[&RegionId(10)].garrison, m.world.regions[&RegionId(11)].garrison), (None, None));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.characters[&CharacterId(101)].garrisoned_in, None);
    assert_eq!(m.world.characters[&CharacterId(101)].position, pos(11, 0), "B's army goes to B's region");
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

/// `DeclareWarAndCallAllies` `0x00B268B0` calls the allies before the war itself (`0x00B26700`):
/// the declarer's ally C joins first (its own war on B, `war_dragged_by_ally` both ways, B's access
/// to C only ended), then A's war, whose third-party loop already sees C hostile to B, so C likes A
/// for declaring war on its enemy.
#[test]
fn the_allies_go_to_war_before_the_declaration() {
    use super::treaties::{event, slot, DiplomaticAction as D};
    let mut m = test_model();
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.apply(CampaignCommand::Diplomacy { a: A, b: C, action: D::Alliance }).unwrap();
    m.relationship_mut(B, C).military_access_turns = 10;
    let events = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let wars: Vec<(FactionId, FactionId)> = events
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::StanceChanged { a, b, stance: Stance::War } => Some((*a, *b)),
            _ => None,
        })
        .collect();
    assert_eq!(wars, [(C, B), (A, B)], "the ally's war comes first");
    let dragged = event("war_dragged_by_ally");
    for (x, y) in [(C, B), (B, C)] {
        let w = m.world.relationships[&(x, y)].attitudes[slot("war")];
        assert_eq!((w.value, w.drift, w.limit), (dragged.value, dragged.drift, dragged.limit), "{x:?}->{y:?}");
    }
    assert_eq!(m.world.relationships[&(C, B)].war_ally, Some(A));
    assert_eq!(m.world.relationships[&(B, C)].war_ally, None);
    // B's access to C ends without an abuse charge (0x00B27250 with an ally).
    assert_eq!(m.world.relationships[&(B, C)].military_access_turns, 0);
    assert_eq!(m.world.relationships[&(B, C)].attitudes[slot("abused_military_access")].value, 0);
    assert!(!m.world.access_abuses.contains_key(&C));
    // C was hostile to B when A's war came: war_against_enemies on C's view of A.
    let e = m.world.relationships[&(C, A)].attitudes[slot("declared_war_against_enemies")];
    assert_eq!(e.value, event("war_against_enemies").value);
    // The access of A and C is saved for the war and becomes indefinite.
    for (x, y) in [(A, C), (C, A)] {
        assert_eq!(m.world.relationships[&(x, y)].military_access_turns, -1);
        assert_eq!(m.world.relationships[&(x, y)].allied_in_war_against.len(), 1);
    }
}

/// An ally already at war with the enemy is not asked: it is listed as joined, and only its access
/// with its ally is saved for the war (`0x00B0CCE0`); no new war, so no new factor.
#[test]
fn an_ally_already_at_war_joins_without_a_new_war() {
    use super::treaties::{slot, DiplomaticAction as D};
    let mut m = test_model();
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.apply(CampaignCommand::Diplomacy { a: A, b: C, action: D::Alliance }).unwrap();
    m.relationship_mut(C, A).military_access_turns = 5;
    m.world.declare_war(C, B).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert!(m.world.relationships.get(&(C, B)).is_none_or(|r| r.attitudes[slot("war")].value == 0));
    let r = &m.world.relationships[&(C, A)];
    assert_eq!((r.military_access_turns, r.allied_in_war_against[0].enemy, r.allied_in_war_against[0].saved_access_turns), (-1, B, 5));
    assert_eq!(m.world.relationships[&(A, C)].military_access_turns, -1);
}

/// A war on the rebels or the pirates gives no one `war_against_enemies` (`0x00B26700` skips it
/// when `0x008CEE00` holds for the target), but a very friendly view still costs the declarer.
#[test]
fn a_war_on_the_rebels_or_pirates_pleases_no_one() {
    use super::treaties::slot;
    let war = |pirates: bool, rebels: bool, value: i32| {
        let mut m = test_model();
        if pirates {
            Arc::make_mut(&mut m.rules).director.no_rival_keys = vec!["test_faction_b".into()];
        }
        if rebels {
            m.world.factions.get_mut(&B).unwrap().key.clear();
        }
        m.relationship_mut(C, B).attitudes[slot("alliance")].value = value;
        m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
        let Some(r) = m.world.relationships.get(&(C, A)) else { return (0, 0) };
        (r.attitudes[slot("declared_war_against_enemies")].value, r.attitudes[slot("declared_war_against_friends")].value)
    };
    assert!(war(false, false, -100).0 > 0, "a hostile bystander likes the declarer");
    assert_eq!(war(true, false, -100), (0, 0));
    assert_eq!(war(false, true, -100), (0, 0));
    assert!(war(true, false, 80).1 < 0, "a very friendly bystander still minds");
}

/// A peace posts `ceasefire_declared` (type 47, `0x009D1F20`) to both sides about the other, the target first,
/// AI or not (`0x00B262C0` has no human test); the `CCQ_MAKE_PEACE` suppress byte (`force_make_protectorate`
/// `0x0097AA20`) posts none.
#[test]
fn peace_posts_ceasefire_messages_unless_suppressed() {
    let messages = |ev: Vec<CampaignEvent>| ev.into_iter().filter(|e| matches!(e, CampaignEvent::Message { .. })).collect::<Vec<_>>();
    let ceasefire = |recipient, other| CampaignEvent::Message { recipient, message: CampaignMessage::CeasefireDeclared { other } };
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let ev = m.apply(CampaignCommand::MakePeace { a: A, b: B, suppress_messages: false }).unwrap();
    assert_eq!(messages(ev), vec![ceasefire(B, A), ceasefire(A, B)]);
    m.world.relationships.get_mut(&(A, B)).unwrap().friendship_turns = 0;
    m.world.relationships.get_mut(&(B, A)).unwrap().friendship_turns = 0;
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let ev = m.apply(CampaignCommand::MakePeace { a: A, b: B, suppress_messages: true }).unwrap();
    assert_eq!(messages(ev), vec![]);
    assert_eq!(m.world.stance(A, B), Stance::Neutral, "a suppressed peace is still a peace");
    assert_eq!(CampaignMessage::CeasefireDeclared { other: A }.type_name(), "ceasefire_declared");
}

/// Timed access left at one turn (2 → 1) posts `military_access_prompt` (type 0x76, `0x00A0E630`) to the
/// holder, titled with the granter (`0x00B29170` → `0x00A0E630`(target, owner)); a 1-turn grant and
/// indefinite access post none, and the granter's records are walked in record order (`0x00B29100`).
#[test]
fn timed_access_with_one_turn_left_posts_the_access_prompt() {
    let prompts = |ev: Vec<CampaignEvent>| -> Vec<(FactionId, FactionId)> {
        ev.iter()
            .filter_map(|e| match e {
                CampaignEvent::Message { recipient, message: CampaignMessage::MilitaryAccessPrompt { granter } } => Some((*recipient, *granter)),
                _ => None,
            })
            .collect()
    };
    let mut m = test_model();
    // C holds no region in the test world: keep it in the game (a target out of it is skipped).
    m.world.faction_details.entry(C).or_default().out_of_game = Some(false);
    m.world.diplomacy_record_order.insert(A, vec![C, B]);
    m.grant_military_access(A, B, 3).unwrap();
    m.grant_military_access(A, C, 2).unwrap();
    assert_eq!(prompts(m.diplomacy_round_end(A)), vec![(C, A)], "C's 2 turns leave 1");
    assert_eq!(prompts(m.diplomacy_round_end(A)), vec![(B, A)], "B's 3 turns leave 1; C's run out silently");
    assert_eq!(prompts(m.diplomacy_round_end(A)), vec![]);
    assert_eq!(m.world.relationships[&(A, B)].military_access_turns, 0);
    // Both at 2 together: record order, C before B.
    m.grant_military_access(A, B, 2).unwrap();
    m.grant_military_access(A, C, 2).unwrap();
    assert_eq!(prompts(m.diplomacy_round_end(A)), vec![(C, A), (B, A)]);
    // A 1-turn grant and indefinite access never leave one turn.
    drop(m.diplomacy_round_end(A));
    m.grant_military_access(A, B, 1).unwrap();
    m.grant_military_access(A, C, -1).unwrap();
    assert_eq!(prompts(m.diplomacy_round_end(A)), vec![]);
    assert_eq!(prompts(m.diplomacy_round_end(A)), vec![]);
    assert_eq!(CampaignMessage::MilitaryAccessPrompt { granter: A }.type_name(), "military_access_prompt");
}

/// Made-up extra factions for the declaration tests: D, E, F in the game, and R, the rebels (no key,
/// outside the turn list as in the exe). Every faction is a major power in the game; the turn list is
/// E, B, A, C, D, F.
const D: FactionId = FactionId(4);
const E: FactionId = FactionId(5);
const F: FactionId = FactionId(6);
const R: FactionId = FactionId(9);

fn war_test_model() -> CampaignModel {
    let mut m = test_model();
    for (id, key) in [(D, "test_faction_d"), (E, "test_faction_e"), (F, "test_faction_f"), (R, "")] {
        m.world.factions.insert(id, super::faction(id, key, 0, GovernmentType::AbsoluteMonarchy));
    }
    for f in [A, B, C, D, E, F, R] {
        let d = m.world.faction_details.entry(f).or_default();
        d.major = Some(true);
        d.out_of_game = Some(false);
    }
    m.world.turn_order = vec![E, B, A, C, D, F];
    m
}

/// A declaration between two major powers posts `war_declared` (type 0x112, `0x009DAFE0`) to every faction
/// of the world's list in none of the call lists (`0x00B268B0` → `0x00B2BEC0`), in list order, AI or not, and
/// never to the rebels (outside the list); none when either side is minor.
#[test]
fn a_war_between_majors_is_announced_to_the_uninvolved() {
    use super::treaties::DiplomaticAction as D_;
    let notices = |ev: &[CampaignEvent]| {
        ev.iter()
            .filter_map(|e| match e {
                CampaignEvent::Message { recipient, message: CampaignMessage::WarDeclared { declarer, target } } => Some((*recipient, *declarer, *target)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut m = war_test_model();
    // C joins A's war (A's ally); D, allied to both, breaks with B when B calls it (B's allies first), then
    // joins A's war when A calls it.
    m.apply(CampaignCommand::Diplomacy { a: A, b: C, action: D_::Alliance }).unwrap();
    m.form_alliance(D, B).unwrap();
    m.form_alliance(D, A).unwrap();
    let ev = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(notices(&ev), vec![(E, A, B), (F, A, B)], "list order E, F; C and D are in the lists, R is not listed");
    assert_eq!(m.world.stance(D, B), Stance::War, "D broke with B, then joined A");
    // The notices come before the war itself (`0x00B26700`).
    let first_notice = ev.iter().position(|e| matches!(e, CampaignEvent::Message { message: CampaignMessage::WarDeclared { .. }, .. })).unwrap();
    let war = ev.iter().position(|e| matches!(e, CampaignEvent::StanceChanged { a: A, b: B, stance: Stance::War })).unwrap();
    assert!(first_notice < war);
    // A minor side: no notice.
    let mut m = war_test_model();
    m.world.faction_details.entry(B).or_default().major = Some(false);
    assert_eq!(notices(&m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap()), vec![]);
    assert_eq!(CampaignMessage::WarDeclared { declarer: A, target: B }.type_name(), "war_declared");
}

/// The war's third-party loop (`0x00B26700`) walks the world's faction list, which never holds the rebels: a
/// rebel faction hostile to the target gets no `war_against_enemies`.
#[test]
fn the_rebels_do_not_judge_a_declaration() {
    use super::treaties::slot;
    let mut m = war_test_model();
    for x in [R, E] {
        m.relationship_mut(x, B).attitudes[slot("alliance")].value = -100;
    }
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let liked = |m: &CampaignModel, x| m.world.relationships.get(&(x, A)).map_or(0, |r| r.attitudes[slot("declared_war_against_enemies")].value);
    assert!(liked(&m, E) > 0, "a listed hostile bystander likes the declarer");
    assert_eq!(liked(&m, R), 0, "the rebels are not listed");
}

/// The allies are called in the order of the side's diplomacy records (`0x00B268B0` walks the record array,
/// stride 0x848), not by faction id.
#[test]
fn the_allies_are_called_in_record_order() {
    let mut m = war_test_model();
    m.form_alliance(C, B).unwrap();
    m.form_alliance(D, B).unwrap();
    let wars = |m: &mut CampaignModel| {
        m.apply(CampaignCommand::DeclareWar { a: A, b: B })
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                CampaignEvent::StanceChanged { a, b, stance: Stance::War } => Some((a, b)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let mut by_record = m.clone();
    by_record.world.diplomacy_record_order.insert(B, vec![F, D, A, C]);
    assert_eq!(wars(&mut by_record), vec![(D, A), (C, A), (A, B)]);
    // Without a loaded order (a faction's records made since), by id.
    assert_eq!(wars(&mut m), vec![(C, A), (D, A), (A, B)]);
}

/// A declaration sets the `WarStartedCharacterFaction` flag (+0x925) of both sides and of a joining ally's
/// war (`0x00B26700`); each faction's own turn end clears it (`0x008BD0F0` at `0x008BD408`), after its
/// `FactionTurnEnd`.
#[test]
fn war_started_lasts_until_the_factions_turn_end() {
    use super::treaties::DiplomaticAction as D_;
    let mut m = war_test_model();
    m.turn.humans = vec![A];
    m.apply(CampaignCommand::Diplomacy { a: A, b: C, action: D_::Alliance }).unwrap();
    m.start_campaign();
    assert_eq!(m.turn.current, Some(A));
    let started = |m: &CampaignModel, f| m.world.faction_details.get(&f).is_some_and(|d| d.war_started);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert!([A, B, C].iter().all(|&f| started(&m, f)));
    assert!(!started(&m, E), "a bystander's flag stays clear");
    m.begin_end_turn();
    let end = m.step().unwrap();
    assert!(end.contains(&CampaignEvent::FactionTurnEnd { faction: A }));
    assert!(started(&m, A), "still set when the FactionTurnEnd handlers run");
    m.step();
    assert!(!started(&m, A), "cleared by A's turn end");
    assert!(started(&m, B) && started(&m, C), "the others keep theirs until their own turn end");
}

/// A [`peace_access_grants`](super::features::CampaignFeatures::peace_access_grants) row (the Peninsular
/// campaign's Iberia → Britain, `0x00B262C0` → `0x00B444B0` with −1) gives the grantee access from the granter,
/// whichever side proposed; a peace between two granters, or with no row, gives none.
#[test]
fn a_peace_access_grant_row_gives_the_grantee_access() {
    use super::features::PeaceAccessGrant;
    let peace = |m: &mut CampaignModel, a, b| {
        m.declare_war_rules(a, b).unwrap();
        m.make_peace_rules(a, b, false).unwrap();
    };
    let access = |m: &CampaignModel, x, y| m.world.relationships.get(&(x, y)).map_or(0, |r| r.military_access_turns);
    let grant = PeaceAccessGrant { granters: vec!["test_faction_a".into(), "test_faction_c".into()], grantee: "test_faction_b".into(), turns: -1 };
    // No row: no access.
    let mut m = test_model();
    peace(&mut m, A, B);
    assert_eq!((access(&m, A, B), access(&m, B, A)), (0, 0));
    // The granter proposes.
    let mut m = test_model();
    std::sync::Arc::make_mut(&mut m.rules).features.peace_access_grants = vec![grant.clone()];
    peace(&mut m, A, B);
    assert_eq!((access(&m, A, B), access(&m, B, A)), (-1, 0), "A grants B indefinite access");
    assert_eq!(m.world.relationships[&(A, B)].military_access_granted, -1);
    // The grantee proposes.
    let mut m = test_model();
    std::sync::Arc::make_mut(&mut m.rules).features.peace_access_grants = vec![PeaceAccessGrant { turns: 10, ..grant.clone() }];
    peace(&mut m, B, C);
    assert_eq!((access(&m, C, B), access(&m, B, C)), (10, 0), "C grants B, the row's length");
    // Two granters: none.
    let mut m = test_model();
    std::sync::Arc::make_mut(&mut m.rules).features.peace_access_grants = vec![grant];
    peace(&mut m, A, C);
    assert_eq!((access(&m, A, C), access(&m, C, A)), (0, 0));
}

/// A rebel region's classes (`0x008CE320` / `0x008CE260`): the government of its first risen army's REBEL_DATA
/// (region +0x220, `0x008CFC20(0)` → army +0x98 → +0x18); with no risen army no class matches. Removing the first
/// army (`0x0088CE30` → `0x0088F310` → `0x008B0150`) makes the next one count.
#[test]
fn a_rebel_regions_classes_are_its_first_risen_armys_government() {
    let mut m = test_model();
    m.world.factions.get_mut(&B).unwrap().key.clear();
    let rules = Arc::make_mut(&mut m.rules);
    rules.government_classes.insert("gov_rep".into(), ("middle".into(), "lower".into()));
    rules.government_classes.insert("gov_abs".into(), ("upper".into(), "lower".into()));
    for id in [2000u32, 2001] {
        m.world.forces.insert(ForceId(id), MilitaryForce { id: ForceId(id), faction: B, commander: None, units: vec![unit(id as i32)], is_navy: false });
    }
    m.world.rebel_army_governments.insert(ForceId(2000), "gov_rep".into());
    m.world.rebel_army_governments.insert(ForceId(2001), "gov_abs".into());
    m.world.regions.get_mut(&RegionId(11)).unwrap().class_bases =
        [("lower", 4), ("middle", -3), ("upper", 6)].iter().map(|&(c, res)| PopulationClassState { resistance: res, ..PopulationClassState::new(c) }).collect();
    let classes = |m: &CampaignModel| {
        let (u, l) = economy::governed_class_factors(m, RegionId(11));
        (u.map(|c| c.class), l.map(|c| c.class))
    };
    // No risen army: no class matches, both previews 0.
    assert_eq!(economy::region_government_classes(&m, &m.world.regions[&RegionId(11)]), None);
    assert_eq!(classes(&m), (None, None));
    assert_eq!(economy::public_order(&m, RegionId(11)), economy::PublicOrder { lower: 0.0, upper: 0.0 });
    // The first risen army's government names the classes, not the second's.
    m.world.region_risen_armies.insert(RegionId(11), vec![ForceId(2000), ForceId(2001)]);
    assert_eq!(classes(&m), (Some("middle".into()), Some("lower".into())));
    // A non-rebel region keeps its governing faction's government.
    assert_eq!(economy::region_government_classes(&m, &m.world.regions[&RegionId(10)]), Some(economy::government_classes(&m, A)));
    // The first army gone: the second counts, and the first's government entry goes with it.
    m.remove_force(ForceId(2000), None);
    assert_eq!(m.world.region_risen_armies[&RegionId(11)], vec![ForceId(2001)]);
    assert!(!m.world.rebel_army_governments.contains_key(&ForceId(2000)));
    assert_eq!(classes(&m), (Some("upper".into()), Some("lower".into())));
    // A first army with no government (none loaded): no class matches.
    m.world.rebel_army_governments.remove(&ForceId(2001));
    assert_eq!(classes(&m), (None, None));
}

/// Position of the first event matching `f` (panics with the list when there is none).
fn event_at(ev: &[CampaignEvent], f: impl Fn(&CampaignEvent) -> bool) -> usize {
    ev.iter().position(f).unwrap_or_else(|| panic!("event missing: {ev:?}"))
}

/// A capture that ends the old owner: `FactionDestroyed` fires inside the owner change
/// (`0x00B449F0` → `0x00A64AC0` → `0x008AB6C0` at `0x00A652A2`), which every capture variant
/// calls before it builds `SettlementOccupied` (`0x00B5874F` / `0x00B589A6` / `0x00B58DA0`); the
/// capture report's choice comes after both.
#[test]
fn a_capture_that_ends_the_owner_reports_its_destruction_before_the_occupation() {
    use super::capture::CaptureChoice;
    for humans in [Vec::new(), vec![A]] {
        let mut m = capture_model(humans.clone());
        set_details(&mut m, A, Some(10), false);
        set_details(&mut m, B, Some(11), false);
        let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
        let destroyed = event_at(&ev, |e| *e == CampaignEvent::FactionDestroyed { faction: B });
        let occupied = event_at(&ev, |e| *e == CampaignEvent::SettlementOccupied { region: RegionId(11), faction: A });
        let report = if humans.is_empty() {
            event_at(&ev, |e| *e == CampaignEvent::CaptureResolved { region: RegionId(11), faction: A, choice: CaptureChoice::Occupy, money: 0 })
        } else {
            event_at(&ev, |e| *e == CampaignEvent::CaptureChoicePending { region: RegionId(11), faction: A })
        };
        assert!(destroyed < occupied && occupied < report, "{ev:?}");
        assert_eq!(ev.iter().filter(|e| matches!(e, CampaignEvent::SettlementOccupied { .. })).count(), 1, "{ev:?}");
    }
}

/// A capture that leaves the old owner in the game posts no `FactionDestroyed`, and
/// `SettlementOccupied` still comes before the capture report. Liberating the region afterwards
/// posts no second `SettlementOccupied` (`0x00889EE0` is built only by the capture variants;
/// `0x00B58A10` forwards to the owner change `0x00B449F0` alone).
#[test]
fn a_capture_that_leaves_the_owner_reports_the_occupation_then_the_choice() {
    use super::capture::CaptureChoice;
    let mut m = capture_model(vec![A]);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::FactionDestroyed { .. })), "{ev:?}");
    let occupied = event_at(&ev, |e| *e == CampaignEvent::SettlementOccupied { region: RegionId(11), faction: A });
    let pending = event_at(&ev, |e| *e == CampaignEvent::CaptureChoicePending { region: RegionId(11), faction: A });
    assert!(occupied < pending, "{ev:?}");
    assert_eq!(m.pending_capture.as_ref().unwrap().liberate, Some(C));
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, C);
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::SettlementOccupied { .. })), "{ev:?}");
    assert!(ev.contains(&CampaignEvent::CaptureResolved { region: RegionId(11), faction: A, choice: CaptureChoice::Liberate, money: 0 }), "{ev:?}");
}

/// The loot choice (`0x008C0310` choice 0, or choice 2 with no liberation target) runs
/// `0x00B541E0`, which posts `CharacterLootedSettlement` (at `0x00B54468`) with the capture report's
/// character: the capturing force's commander (report +0x8, from force +0x74 in `0x00B58560` /
/// `0x00B58C00`), after the option is applied. Occupy (`0x00B582E0`) and a liberation with a
/// target (`0x00B4F090`) post no loot event, nor does a loot with no commander (`0x008F9EE0` gate).
#[test]
fn looting_posts_character_looted_settlement_with_the_capturing_commander() {
    use super::capture::CaptureChoice;
    let looted = |ev: &[CampaignEvent]| ev.iter().filter(|e| matches!(e, CampaignEvent::CharacterLootedSettlement { .. })).count();
    // Loot: after the option is applied, before the report closes.
    let mut m = capture_model(vec![A]);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(looted(&ev), 0, "the choice waits: {ev:?}");
    assert_eq!(m.pending_capture.as_ref().unwrap().looter, Some(CharacterId(100)));
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
    let loot = event_at(&ev, |e| *e == CampaignEvent::CharacterLootedSettlement { character: CharacterId(100) });
    let resolved = event_at(&ev, |e| matches!(e, CampaignEvent::CaptureResolved { choice: CaptureChoice::Loot, .. }));
    assert!(loot < resolved, "{ev:?}");
    assert_eq!(looted(&ev), 1, "{ev:?}");
    // Occupy: none.
    let mut m = capture_model(vec![A]);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert_eq!(looted(&ev), 0, "{ev:?}");
    // Liberate with a target: none.
    let mut m = capture_model(vec![A]);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(looted(&ev), 0, "{ev:?}");
    // Liberate with no target loots, and so posts the event.
    let mut m = capture_model(vec![A]);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterLootedSettlement { character: CharacterId(100) }), "{ev:?}");
    // A capture with no force (a scripted capture) has no commander: the exe posts the event, but its
    // +0xB8 check (0x008F9EE0) fails on the null character, so 0x00E30700 never hands it to Lua.
    let mut m = capture_model(vec![A]);
    m.capture_by_force(RegionId(11), A, None);
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CaptureResolved { choice: CaptureChoice::Loot, .. })), "{ev:?}");
    assert_eq!(looted(&ev), 0, "{ev:?}");
}

/// A liberation posts `FactionBecomesLiberationProtectorate` first (`0x00B4F0FE` / `0x00B5089D`, before
/// the expulsion `0x008D9DC0` and the owner change `0x00B58A10`), once, with the liberator (the
/// region's owner when it is posted); the liberated faction's return (its stance changes) and the
/// capture report come after it. Loot and occupy post none.
#[test]
fn a_liberation_posts_the_protectorate_event_before_the_owner_change() {
    use super::capture::CaptureChoice;
    let start = |m: &mut CampaignModel| {
        m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
        set_details(m, C, None, true);
        m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap()
    };
    let mut m = capture_model(vec![A]);
    let ev = start(&mut m);
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::FactionBecomesLiberationProtectorate { .. })), "{ev:?}");
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(ev[0], CampaignEvent::FactionBecomesLiberationProtectorate { faction: A }, "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, CampaignEvent::FactionBecomesLiberationProtectorate { .. })).count(), 1);
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::StanceChanged { .. })), "C came back: {ev:?}");
    assert_eq!(
        ev.last(),
        Some(&CampaignEvent::CaptureResolved { region: RegionId(11), faction: A, choice: CaptureChoice::Liberate, money: 0 })
    );
    assert!(m.in_the_game(C) && m.world.regions[&RegionId(11)].owner == C);
    assert_eq!(CampaignEvent::FactionBecomesLiberationProtectorate { faction: A }.script_name(), Some("FactionBecomesLiberationProtectorate"));
    for choice in [CaptureChoice::Loot, CaptureChoice::Occupy] {
        let mut m = capture_model(vec![A]);
        start(&mut m);
        let ev = m.apply(CampaignCommand::ChooseCapture { choice }).unwrap();
        assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::FactionBecomesLiberationProtectorate { .. })), "{choice:?}: {ev:?}");
    }
}
