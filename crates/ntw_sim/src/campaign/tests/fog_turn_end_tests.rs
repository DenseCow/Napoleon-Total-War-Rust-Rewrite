//! Campaign model tests: fog turn end.

use super::*;

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

    // 3. The labels layer's test is the hide set alone (`0x009BAA90` → `0x00B11E70`), not `sees`
    //    nor "seen once": explored, in sight and never seen all keep their labels; only the
    //    hidden cell (10, 10) loses it, though it is also visible.
    let explored_at = m.cell_centre(3, 4).unwrap();
    assert!(!m.hides_map_label(A, explored_at), "seen once: labelled");
    assert!(!m.sees(A, explored_at), "...but not currently in sight");
    assert!(!m.hides_map_label(A, m.cell_centre(60, 45).unwrap()), "in sight now: labelled");
    assert!(!m.hides_map_label(A, m.cell_centre(0, 0).unwrap()), "never seen: still labelled");
    assert!(m.hides_map_label(A, m.cell_centre(10, 10).unwrap()), "in the hide set: no label");
    assert!(!m.hides_map_label(A, (1.0e6, -1.0e6)), "off the grid: the tree lookup fails, labelled");
    assert!(!m.hides_map_label(B, m.cell_centre(10, 10).unwrap()), "no shroud: the test is skipped");
    m.world.shrouds.get_mut(&A).unwrap().active = false;
    assert!(!m.hides_map_label(A, m.cell_centre(10, 10).unwrap()), "shroud off (+0x50 clear): labelled");
    m.world.shrouds.get_mut(&A).unwrap().active = true;

    // 4. `fog_state == Visible` is exactly `sees` -- on the grid, off it, with and without a
    //    shroud.
    let probes = [m.cell_centre(3, 4).unwrap(), m.cell_centre(60, 45).unwrap(), m.cell_centre(10, 10).unwrap(), (0.0, 0.0), (1.0e6, -1.0e6)];
    for f in [A, B] {
        for p in probes {
            assert_eq!(m.fog_state(f, p) == FogState::Visible, m.sees(f, p), "{f:?} at {p:?}");
        }
    }
    assert!(m.sees(B, (1.0e6, -1.0e6)), "no shroud: even off the grid");
    m.world.sight_grid = None;
    assert_eq!(m.cell_centre(0, 0), None, "no grid: no cell, and no panic");
    assert_eq!(m.fog_state(A, (0.0, 0.0)), FogState::Visible, "no grid: like `sees`");
    assert!(!m.hides_map_label(A, (0.0, 0.0)), "no grid: no cell, labelled");
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
        star_rating: 0,
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
        star_rating: 0,
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
    m.diplomacy_round_end(B, &mut Vec::new());
    assert_eq!(m.world.relationships[&(B, A)].attitudes[i].value, -88);
    m.diplomacy_round_end(C, &mut Vec::new());
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 68);
    // ...and holds it there once it is on the limit (-90 -> -30 is 30 turns, 70 -> 15 is 28).
    for _ in 0..40 {
        m.diplomacy_round_end(B, &mut Vec::new());
        m.diplomacy_round_end(C, &mut Vec::new());
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
    m.diplomacy_round_end(B, &mut Vec::new());
    assert_eq!(m.world.relationships[&(B, A)].attitudes[i].value, -88);
    // C is out of the game: written all the same, but its round end moves nothing.
    assert_eq!(m.world.relationships[&(C, A)].attitudes[i].value, 70);
    assert!(m.world.relationships[&(C, A)].attitudes[i].limited);
    m.diplomacy_round_end(C, &mut Vec::new());
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
        economy::bankrupt_desertion(&mut m, B, 0, 20, &mut Vec::new());
        (m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).collect::<Vec<_>>(), m.rng.unit_float())
    };
    let (alone, next_alone) = run(&["test_unit"]);
    assert!(alone[0] < 80);
    let (mixed, next_mixed) = run(&["test_guard", "test_cuirassiers", "test_general", "test_elephants", "test_unit"]);
    assert_eq!(mixed, vec![80, 80, 80, 80, alone[0]]);
    assert_eq!(next_mixed, next_alone, "the exempt units drew no random number");
}
