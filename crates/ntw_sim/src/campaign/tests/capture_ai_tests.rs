//! Campaign model tests: the AI's loot / occupy / liberate choice after a capture (`0x00D8C8B0`,
//! [`super::capture_ai`]), each branch on the MADE-UP world with MADE-UP tunables (percent, as the table holds them).

use super::*;
use super::capture::{CaptureChoice, CapturePreview};
use super::capture_ai::{capture_choice_original, TUNABLES};
use super::details::{AttitudeFactor, Relationship};
use super::features::AiCaptureLiberation;
use super::personalities::AiPersonalities;

const D: FactionId = FactionId(4);
const BASE: &str = TUNABLES[0];
const PCT: &str = TUNABLES[1];
const ECON: &str = TUNABLES[2];
const DIPLO: &str = TUNABLES[3];
const GROUPING: &str = TUNABLES[4];
const FLOOR: &str = TUNABLES[5];
const CEILING: &str = TUNABLES[6];

/// [`test_model`] whose default personality holds `tunables` (the others read 0), in percent as the table holds them.
fn model_with(tunables: &[(&str, f32)]) -> CampaignModel {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    let mut p = AiPersonalities { default: "default".into(), ..Default::default() };
    p.by_key.entry("default".into()).or_default().extend(tunables.iter().map(|&(k, v)| (k.to_owned(), v)));
    rules.ai_personalities = p;
    m.rules = Arc::new(rules);
    m
}

fn preview(region: u32, faction: FactionId, liberate: Option<FactionId>) -> CapturePreview {
    CapturePreview {
        region: RegionId(region),
        faction,
        surrender: false,
        loot: Default::default(),
        occupy: Default::default(),
        liberate,
        looter: None,
    }
}

/// The choice and whether it drew from the RNG.
fn choose(m: &mut CampaignModel, p: &CapturePreview, previous: FactionId) -> (CaptureChoice, bool) {
    let before = m.cai_rng;
    let c = capture_choice_original(m, p, Some(previous));
    (c, m.cai_rng != before)
}

/// What step 6's draw gives for loot chance `p` on `m`'s RNG.
fn drawn(m: &CampaignModel, p: f32) -> CaptureChoice {
    let mut rng = m.cai_rng;
    if p < rng.float_range(0.0, 1.0) { CaptureChoice::Occupy } else { CaptureChoice::Loot }
}

#[test]
fn a_capture_without_a_previous_owner_is_occupied() {
    let mut m = model_with(&[(CEILING, 0.0)]);
    let before = m.cai_rng;
    assert_eq!(capture_choice_original(&mut m, &preview(11, A, None), None), CaptureChoice::Occupy);
    assert_eq!(m.cai_rng, before, "no draw");
}

#[test]
fn the_floor_occupies_and_the_ceiling_loots_without_a_draw() {
    // p = base 50 % = 0.5 (region 11 borders nothing of A's, grouping 0).
    let mut m = model_with(&[(BASE, 50.0), (FLOOR, 50.0), (CEILING, 100.0)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B), (CaptureChoice::Occupy, false), "floor >= p");
    let mut m = model_with(&[(BASE, 50.0), (FLOOR, 10.0), (CEILING, 50.0)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B), (CaptureChoice::Loot, false), "p >= ceiling");
    // Clamped to [0, 1] before both: a base of 300 % is 1, at a ceiling of 1.
    let mut m = model_with(&[(BASE, 300.0), (FLOOR, 10.0), (CEILING, 100.0)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B), (CaptureChoice::Loot, false));
    // A NaN chance clamps to 0, at a floor of 0.
    let mut m = model_with(&[(BASE, f32::NAN), (CEILING, 100.0)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B), (CaptureChoice::Occupy, false));
}

#[test]
fn between_the_floor_and_the_ceiling_one_draw_decides() {
    for base in [5.0, 50.0, 95.0] {
        let mut m = model_with(&[(BASE, base), (FLOOR, 1.0), (CEILING, 99.0)]);
        let want = drawn(&m, base * 0.01);
        assert_eq!(choose(&mut m, &preview(11, A, None), B), (want, true), "base {base}");
    }
}

#[test]
fn a_region_away_from_the_capturer_adds_the_grouping_modifier() {
    let tunables = [(BASE, 25.0), (GROUPING, 50.0), (FLOOR, 30.0), (CEILING, 70.0)];
    // 0.25 + 0.5 = 0.75 >= the ceiling.
    let mut m = model_with(&tunables);
    assert_eq!(choose(&mut m, &preview(11, A, None), B).0, CaptureChoice::Loot);
    // Region 11 next to A's region 10: 0.25 <= the floor.
    let mut m = model_with(&tunables);
    m.world.region_neighbours.insert(RegionId(11), vec![RegionId(10)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B).0, CaptureChoice::Occupy);
}

#[test]
fn spending_above_income_adds_the_economic_modifier() {
    let tunables = [(BASE, 10.0), (PCT, 50.0), (ECON, 60.0), (FLOOR, 20.0), (CEILING, 65.0)];
    let record = |income: i32| {
        let mut r = [0; 25];
        r[13] = 60; // spending, categories 13..=24
        r[24] = 40;
        r[0] = income; // income, categories 0..=12
        r
    };
    // 100 × 0.5 = 50 > 49: p = 0.6 + 0.1 = 0.7 >= the ceiling.
    let mut m = model_with(&tunables);
    m.world.economy_history.insert(A, vec![record(10_000), record(49)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B).0, CaptureChoice::Loot, "the running (last) record");
    // 50 > 50 fails: p = 0.1 <= the floor.
    let mut m = model_with(&tunables);
    m.world.economy_history.insert(A, vec![record(50)]);
    assert_eq!(choose(&mut m, &preview(11, A, None), B).0, CaptureChoice::Occupy);
}

#[test]
fn a_bad_attitude_towards_the_previous_owner_adds_the_diplomatic_modifier_per_20() {
    let tunables = [(DIPLO, 30.0), (FLOOR, 35.0), (CEILING, 55.0)];
    let with_attitude = |value: i32| {
        let mut m = model_with(&tunables);
        let r = Relationship { attitudes: vec![AttitudeFactor { value, ..Default::default() }], ..Default::default() };
        m.world.relationships.insert((A, B), r);
        m
    };
    // -45 / 20 = -2 (truncated): 2 × 0.3 = 0.6 >= the ceiling.
    assert_eq!(choose(&mut with_attitude(-45), &preview(11, A, None), B).0, CaptureChoice::Loot);
    // -39 / 20 = -1: 0.3 <= the floor.
    assert_eq!(choose(&mut with_attitude(-39), &preview(11, A, None), B).0, CaptureChoice::Occupy);
    // A good attitude adds nothing.
    assert_eq!(choose(&mut with_attitude(200), &preview(11, A, None), B).0, CaptureChoice::Occupy);
}

#[test]
fn rebels_skip_the_personality() {
    // The floor would occupy without a draw; the rebels' fixed chances draw instead.
    let tunables = [(FLOOR, 100.0)];
    let mut m = model_with(&tunables);
    m.world.factions.get_mut(&C).unwrap().key = String::new();
    let want = drawn(&m, 0.7);
    assert_eq!(choose(&mut m, &preview(11, C, None), B), (want, true), "a rebel capturer: 0.7");
    let mut m = model_with(&tunables);
    m.world.factions.get_mut(&C).unwrap().key = String::new();
    let want = drawn(&m, 0.2);
    assert_eq!(choose(&mut m, &preview(11, A, None), C), (want, true), "taken from the rebels: 0.2");
}

/// A: liberator, B: contested liberator, C: target, D: rival. B holds the captured region 11 and region 12;
/// D holds 10 and 13, C holds 14. 11 borders D's 10 and C's 14, B's 12 borders D's 13.
fn liberation_model() -> CampaignModel {
    let mut m = model_with(&[(FLOOR, 100.0)]);
    let mut rules = (*m.rules).clone();
    rules.features.ai_capture_liberation = Some(AiCaptureLiberation {
        liberator: "test_faction_a".into(),
        contested_liberator: "test_faction_b".into(),
        target: "test_faction_c".into(),
        rival: "test_faction_d".into(),
    });
    m.rules = Arc::new(rules);
    m.world.factions.insert(D, faction(D, "test_faction_d", 0, GovernmentType::AbsoluteMonarchy));
    for (id, owner) in [(13, D), (14, C)] {
        let r = region(id, owner, 1000, 0);
        m.world.regions.insert(r.id, r);
    }
    for (id, owner) in [(10, D), (11, B), (12, B)] {
        m.world.regions.get_mut(&RegionId(id)).unwrap().owner = owner;
    }
    for (a, b) in [(11, 10), (11, 14), (12, 13)] {
        m.world.region_neighbours.entry(RegionId(a)).or_default().push(RegionId(b));
        m.world.region_neighbours.entry(RegionId(b)).or_default().push(RegionId(a));
    }
    m
}

#[test]
fn the_liberator_always_liberates_the_target() {
    let mut m = liberation_model();
    assert_eq!(choose(&mut m, &preview(11, A, Some(C)), B), (CaptureChoice::Liberate, false));
    // Another faction offered: no liberation (the floor occupies).
    let mut m = liberation_model();
    assert_eq!(choose(&mut m, &preview(11, A, Some(D)), B), (CaptureChoice::Occupy, false));
    // Without the campaign's rule: no liberation.
    let mut m = liberation_model();
    let mut rules = (*m.rules).clone();
    rules.features.ai_capture_liberation = None;
    m.rules = Arc::new(rules);
    assert_eq!(choose(&mut m, &preview(11, A, Some(C)), B), (CaptureChoice::Occupy, false));
}

#[test]
fn the_contested_liberator_draws_between_its_and_the_targets_counts() {
    // S = C's 1 region; B = B's 2 + 1 for R's boundary to C's 14 = 3: liberate when 1..=4 draws ≤ 3.
    let mut m = liberation_model();
    let mut rng = m.cai_rng;
    let want = if rng.int_range(1, 4) <= 3 { CaptureChoice::Liberate } else { CaptureChoice::Occupy };
    assert_eq!(choose(&mut m, &preview(11, B, Some(C)), D), (want, true));
    // The target out of the game: liberate without a draw.
    let mut m = liberation_model();
    m.world.faction_details.entry(C).or_default().out_of_game = Some(true);
    assert_eq!(choose(&mut m, &preview(11, B, Some(C)), D), (CaptureChoice::Liberate, false));
}

#[test]
fn the_contested_liberator_needs_the_rival_on_both_borders() {
    // R does not border the rival.
    let mut m = liberation_model();
    m.world.regions.get_mut(&RegionId(10)).unwrap().owner = A;
    assert_eq!(choose(&mut m, &preview(11, B, Some(C)), D), (CaptureChoice::Occupy, false));
    // No other region of the capturer borders the rival.
    let mut m = liberation_model();
    m.world.regions.get_mut(&RegionId(13)).unwrap().owner = A;
    assert_eq!(choose(&mut m, &preview(11, B, Some(C)), D), (CaptureChoice::Occupy, false));
}

#[test]
fn a_contested_liberator_with_one_region_needs_the_targets_army_inside() {
    // Called directly with R still A's and no boundary to C: B = B's 1 region (12).
    let mut m = liberation_model();
    m.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
    m.world.region_neighbours.get_mut(&RegionId(11)).unwrap().retain(|&r| r != RegionId(14));
    assert_eq!(choose(&mut m, &preview(11, B, Some(C)), D), (CaptureChoice::Occupy, false), "falls through to the floor");
    // C's army with units in R: the draw (S = 1, B = 1).
    let mut c = character(103, C, CharacterKind::General, 30);
    c.garrisoned_in = Some(RegionId(11));
    m.world.characters.insert(c.id, c);
    m.world.forces.insert(ForceId(1003), MilitaryForce { id: ForceId(1003), faction: C, commander: Some(CharacterId(103)), units: vec![unit(9)], is_navy: false });
    let mut rng = m.cai_rng;
    let want = if rng.int_range(1, 2) <= 1 { CaptureChoice::Liberate } else { CaptureChoice::Occupy };
    assert_eq!(choose(&mut m, &preview(11, B, Some(C)), D), (want, true));
}

#[test]
fn an_ai_capture_asks_the_seam_with_the_previous_owner() {
    // A (AI) takes B's region 11: the ceiling loots (base 0 >= 0) and the floor is below it.
    let mut m = model_with(&[(FLOOR, -100.0)]);
    m.turn.humans = Vec::new();
    let events = m.capture_by_force(RegionId(11), A, Some(ForceId(1000)));
    let resolved = events.iter().find_map(|e| match e {
        CampaignEvent::CaptureResolved { choice, .. } => Some(*choice),
        _ => None,
    });
    assert_eq!(resolved, Some(CaptureChoice::Loot));
    // The seam gets the owner before the capture (B), not the new owner: a MADE-UP mod rule liberates then.
    let mut m = model_with(&[]);
    m.turn.humans = Vec::new();
    let mut rules = (*m.rules).clone();
    let maker: crate::seam::Maker<super::seams::CaptureAiChoice> = Arc::new(|_original| {
        Arc::new(|_: &mut CampaignModel, _: &CapturePreview, previous: Option<FactionId>, _: &mut super::mod_state::ModWrites| {
            if previous == Some(B) { CaptureChoice::Liberate } else { CaptureChoice::Occupy }
        })
    });
    rules.seams.capture_ai_choice.apply("test_mod.liberate_from_b", &maker);
    m.rules = Arc::new(rules);
    let events = m.capture_by_force(RegionId(11), A, Some(ForceId(1000)));
    // No liberation target: the apply step loots instead (`0x008C0310`).
    let resolved = events.iter().find_map(|e| match e {
        CampaignEvent::CaptureResolved { choice, .. } => Some(*choice),
        _ => None,
    });
    assert_eq!(resolved, Some(CaptureChoice::Loot));
}

/// The choice the capture of region 11 by `m`'s A with force 1000 resolves to.
fn captured(m: &mut CampaignModel) -> Option<CaptureChoice> {
    m.capture_by_force(RegionId(11), A, Some(ForceId(1000))).iter().find_map(|e| match e {
        CampaignEvent::CaptureResolved { choice, .. } => Some(*choice),
        _ => None,
    })
}

/// [`model_with`] where the AI's choice would loot (the ceiling of 0 is at or below any chance) and force 1000
/// is a risen army with `side` (MADE-UP).
fn rebel_capture(side: RebelArmySide) -> CampaignModel {
    let mut m = model_with(&[(FLOOR, -100.0)]);
    m.turn.humans = Vec::new();
    m.world.rebel_army_sides.insert(ForceId(1000), side);
    m
}

#[test]
fn a_risen_army_of_a_faction_in_the_game_occupies_without_the_choice() {
    // +0x9B (`0x00B4D9E0`): the army rises for C, which the registry finds and which is in the game.
    let side = RebelArmySide { faction: "test_faction_c".into(), link: 0 };
    let mut m = rebel_capture(side.clone());
    m.world.faction_details.entry(C).or_default().out_of_game = Some(false);
    let before = m.cai_rng;
    assert_eq!(captured(&mut m), Some(CaptureChoice::Occupy));
    assert_eq!(m.cai_rng, before, "no draw: the AI's choice is skipped");
    // C out of the game, or a faction the registry does not hold: the choice runs (and loots).
    let mut m = rebel_capture(side);
    m.world.faction_details.entry(C).or_default().out_of_game = Some(true);
    assert_eq!(captured(&mut m), Some(CaptureChoice::Loot));
    let mut m = rebel_capture(RebelArmySide { faction: "portugese_rebels".into(), link: 0 });
    assert_eq!(captured(&mut m), Some(CaptureChoice::Loot));
    // Without a REBEL_DATA block the force never skips.
    let mut m = model_with(&[(FLOOR, -100.0)]);
    m.turn.humans = Vec::new();
    assert_eq!(captured(&mut m), Some(CaptureChoice::Loot));
}

#[test]
fn a_risen_army_with_a_link_taking_its_own_region_occupies() {
    // +0x9A (`0x00B4DA40`): +0x34 set and the army rose in the captured region.
    let side = RebelArmySide { faction: "portugese_rebels".into(), link: 7 };
    let mut m = rebel_capture(side.clone());
    m.world.region_risen_armies.insert(RegionId(11), vec![ForceId(1000)]);
    assert_eq!(captured(&mut m), Some(CaptureChoice::Occupy));
    // Risen elsewhere, or no link: the choice runs.
    let mut m = rebel_capture(side);
    m.world.region_risen_armies.insert(RegionId(10), vec![ForceId(1000)]);
    assert_eq!(captured(&mut m), Some(CaptureChoice::Loot));
    let mut m = rebel_capture(RebelArmySide { faction: "portugese_rebels".into(), link: 0 });
    m.world.region_risen_armies.insert(RegionId(11), vec![ForceId(1000)]);
    assert_eq!(captured(&mut m), Some(CaptureChoice::Loot));
}

#[test]
fn the_skip_flags_occupy_for_a_human_too() {
    let mut m = rebel_capture(RebelArmySide { faction: "test_faction_c".into(), link: 0 });
    m.world.faction_details.entry(C).or_default().out_of_game = Some(false);
    m.turn.humans = vec![A];
    assert_eq!(captured(&mut m), Some(CaptureChoice::Occupy));
    assert!(m.pending_capture.is_none(), "no choice waits");
}

#[test]
fn the_choice_draws_on_the_cai_stream_not_the_campaign_rng() {
    let mut m = model_with(&[]);
    m.world.factions.get_mut(&C).unwrap().key = String::new();
    m.cai_rng = crate::rng::CaRng::new(0x1234_5678);
    let (campaign, mut cai) = (m.rng, m.cai_rng);
    let want = if 0.7 < cai.float_range(0.0, 1.0) { CaptureChoice::Occupy } else { CaptureChoice::Loot };
    assert_eq!(capture_choice_original(&mut m, &preview(11, C, None), Some(B)), want);
    assert_eq!((m.rng, m.cai_rng), (campaign, cai), "one CAI draw, the campaign RNG untouched");
}

#[test]
fn a_new_campaign_reseeds_the_cai_stream_from_one_campaign_step() {
    let mut m = test_model();
    assert_eq!(m.cai_rng, crate::rng::CaRng::new(CAI_RNG_SEED), "the loader's preset");
    let mut campaign = m.rng;
    let seed = campaign.next16();
    m.reseed_cai_rng();
    assert_eq!((m.rng, m.cai_rng), (campaign, crate::rng::CaRng::new(seed)));
}
