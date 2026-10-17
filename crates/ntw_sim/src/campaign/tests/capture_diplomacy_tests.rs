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
    let mut no_capital = m.clone();
    set_details(&mut m, A, Some(10), false);
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, C);
    // The liberation's owner change (0x00B58A10 → 0x00A64AC0, fourth argument 0) expels the
    // capturer's army from the region (0x008D9DC0); a capturer without a capital is not in that
    // pass (the world's faction list, 0x00A64AC0) and stays inside, a foreign garrison.
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(11)].garrison, None);
    no_capital.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert_eq!(no_capital.world.regions[&RegionId(11)].garrison, Some(ForceId(1000)));
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
    m.apply(CampaignCommand::MakePeace { a: A, b: B }).unwrap();
    // First backstab, 4 friendship turns: n = 1 → value −3·1·4, limit −5·1; then n + 1.
    war(&mut m, 4);
    assert_eq!(peace_treaty(&m), (-12, 1, -5));
    assert_eq!(m.world.treaty_breaks[&A], 2);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[slot("abused_military_access")].value, -40, "second abuse: −30 + 1 × (−10)");
    assert_eq!(m.world.access_abuses[&A], 2);
    m.apply(CampaignCommand::MakePeace { a: A, b: B }).unwrap();
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
