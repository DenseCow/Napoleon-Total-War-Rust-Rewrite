//! Campaign model tests: agents.

use super::*;

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

/// The yearly death checks draw in the order of the faction's character list (+0x768, `0x008BC650`),
/// not in id order: two characters listed the other way round get each other's draws, and the dead
/// die in list order.
#[test]
fn the_yearly_death_checks_draw_in_list_order() {
    // C plays last (turn order B, A, C), so the draws before its own are the same in every run; 200
    // and 201 are its only characters with a birth date.
    let model = |seed: u32, ages: (u32, u32), order: Vec<CharacterId>| {
        let mut m = test_model();
        m.rng = CaRng::new(seed);
        m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
        let year = m.calendar.date.year;
        for (id, age) in [(200, ages.0), (201, ages.1)] {
            m.world.characters.insert(CharacterId(id), character(id, C, CharacterKind::Minister, 0));
            m.world.character_details.entry(CharacterId(id)).or_default().birth = Some(Date { year: year - age, season: 0, month: 0, half: HALF_EARLY });
        }
        m.world.character_order = order;
        m
    };
    let died = |mut m: CampaignModel| super::characters::yearly_character_pass(&mut m).died;
    let swap = |c: CharacterId| if c == CharacterId(200) { CharacterId(201) } else { CharacterId(200) };
    let listed_order = || vec![CharacterId(201), CharacterId(200)];
    let mut differs = false;
    for seed in 0..64 {
        // Listed 201 first: the same draws as 200 first (id order) with the ages swapped.
        let listed = died(model(seed, (70, 66), listed_order()));
        let swapped = died(model(seed, (66, 70), Vec::new()));
        assert_eq!(listed.iter().map(|c| swap(*c)).collect::<Vec<_>>(), swapped, "seed {seed}");
        let mut by_id = died(model(seed, (70, 66), Vec::new()));
        let mut sorted = listed.clone();
        sorted.sort();
        by_id.sort();
        differs |= sorted != by_id;
    }
    assert!(differs, "the order changes who dies for some seed");
    // Both certain to die (age 100: chance 1): in list order.
    assert_eq!(died(model(1, (100, 100), listed_order())), listed_order());
}

/// charflag: the family year runs for a faction still in the game (`0x008BC650` tests faction +0x824 at
/// `0x008BC6D0`), with or without a region, and not for one out of the game, even holding a region
/// (it was gated on owning a region).
#[test]
fn the_family_year_follows_the_out_of_game_flag() {
    use super::family::{Family, FamilyMember, LEADER};
    let leader_age = |out: bool, region: bool| {
        let mut m = test_model();
        m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
        if region {
            m.world.regions.get_mut(&RegionId(11)).unwrap().owner = C;
        }
        let mut family = Family { members: vec![FamilyMember::default(); 10], ordinals: Vec::new(), female_heirs: false };
        family.members[LEADER] = FamilyMember { names: vec!["Karl".into()], male: true, exists: true, age: 60, ..Default::default() };
        let d = m.world.faction_details.entry(C).or_default();
        d.family = Some(family);
        d.out_of_game = Some(out);
        super::characters::yearly_character_pass(&mut m);
        m.world.faction_details[&C].family.as_ref().unwrap().members[LEADER].age
    };
    assert_eq!(leader_age(false, false), 61, "in the game without a region");
    assert_eq!(leader_age(false, true), 61);
    assert_eq!(leader_age(true, false), 60, "out of the game");
    assert_eq!(leader_age(true, true), 60, "out of the game holding a region");
}

/// charflag: the rebels have no family year even while in the game (`0x008BC650` tests `0x008CEEF0`
/// before faction +0x824).
#[test]
fn the_rebels_have_no_family_year() {
    use super::family::{Family, FamilyMember, LEADER};
    let leader_age = |rebel: bool| {
        let mut m = test_model();
        m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
        if rebel {
            m.world.factions.get_mut(&C).unwrap().key.clear();
        }
        let mut family = Family { members: vec![FamilyMember::default(); 10], ordinals: Vec::new(), female_heirs: false };
        family.members[LEADER] = FamilyMember { names: vec!["Karl".into()], male: true, exists: true, age: 60, ..Default::default() };
        let d = m.world.faction_details.entry(C).or_default();
        d.family = Some(family);
        d.out_of_game = Some(false);
        assert_eq!(m.is_rebel_faction(C), rebel);
        super::characters::yearly_character_pass(&mut m);
        m.world.faction_details[&C].family.as_ref().unwrap().members[LEADER].age
    };
    assert_eq!(leader_age(false), 61, "in the game, not the rebels");
    assert_eq!(leader_age(true), 60, "the rebels, in the game");
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
    // The picker's weapon (0xE in `0x00922940`): the target's better one by his clamped skills,
    // pistols on a tie (105: pistols 1, swords 1).
    use super::agents::duel_quoted_weapon;
    assert_eq!(duel_quoted_weapon(&m, CharacterId(105)), Weapon::Pistols);
    m.world.character_details.get_mut(&CharacterId(105)).unwrap().attributes[1].1 = 2;
    assert_eq!(duel_quoted_weapon(&m, CharacterId(105)), Weapon::Swords);
    // Clamped to 0..=9 first: 12 and 9 tie, so pistols.
    m.world.character_details.get_mut(&CharacterId(105)).unwrap().attributes[1].1 = 12;
    m.world.character_details.get_mut(&CharacterId(105)).unwrap().attributes[0].1 = 9;
    assert_eq!(duel_quoted_weapon(&m, CharacterId(105)), Weapon::Pistols);
    let a = &mut m.world.character_details.get_mut(&CharacterId(105)).unwrap().attributes;
    (a[0].1, a[1].1) = (1, 1);
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
    // An unseen sabotage of B by A: B blames the faction it likes least other than A (C), which gets
    // the victim values; every faction but the victim B and the blamed C (so the culprit A too) gives
    // C the third-party values (`0x00B0D800` compares with the victim).
    let mut m = agents_model();
    with_relationships(&mut m, &[((B, A), -10), ((B, C), -100)]);
    m.blamed_misdeed(A, B, Misdeed::Sabotage);
    let f = &m.world.relationships[&(B, C)].attitudes[18];
    assert_eq!((f.value, f.drift), (-20, 2));
    let f = &m.world.relationships[&(A, C)].attitudes[18];
    assert_eq!((f.value, f.drift), (-2, 1));
    assert_eq!(m.world.relationships[&(B, A)].attitudes[18].value, 0);
}

/// treatyorder: the victim walks its records in their creation order (`0x00B0D740` indexes them through
/// `0x00B64CB0`) and takes the first one whoever it points at, so the order decides who is blamed: with
/// B's records listed C first, C (-5) is blamed and the culprit A (-100) is skipped; listed A first (also
/// id order, a world with no list), A is taken first and C, liked more, never replaces it.
#[test]
fn an_unseen_misdeed_is_blamed_in_record_order() {
    use super::agents::Misdeed;
    let mut m = agents_model();
    with_relationships(&mut m, &[((B, A), -100), ((B, C), -5)]);
    m.world.diplomacy_record_order.insert(B, vec![C, A]);
    m.blamed_misdeed(A, B, Misdeed::Sabotage);
    assert_eq!(m.world.relationships[&(B, C)].attitudes[18].value, -20);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[18].value, 0);
    let mut m = agents_model();
    with_relationships(&mut m, &[((B, A), -100), ((B, C), -5)]);
    m.blamed_misdeed(A, B, Misdeed::Sabotage);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[18].value, -20);
    assert_eq!(m.world.relationships[&(B, C)].attitudes[18].value, 0);
}

#[test]
fn misdeeds_against_the_rebels_change_nothing() {
    use super::agents::Misdeed;
    // The rebels (no faction record, `0x008CEEF0`) as the victim: no attitude change, no broadcast.
    let mut m = agents_model();
    with_relationships(&mut m, &[]);
    m.world.factions.get_mut(&B).unwrap().key.clear();
    m.director.beliefs.insert(C, Default::default());
    m.detected_misdeed(A, B, Misdeed::Assassination);
    m.blamed_misdeed(A, B, Misdeed::Assassination);
    assert_eq!(m.world.relationships[&(B, A)].attitudes[14].value, 0);
    assert_eq!(m.world.relationships[&(C, A)].attitudes[14].value, 0);
    assert!(m.director.beliefs.contains_key(&C), "no diplomacy-changed broadcast");
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
    // S = 0: the floor, 5, not "no chance" (`0x00922C90` jumps to its return-5 at `0x00922D6A`),
    // so the building stays a target in the picker.
    agent_details(&mut m, 103, &[("can_sabotage", 1, "subterfuge")], &[("subterfuge", 0)]);
    assert_eq!(building_sabotage_chance(&m, CharacterId(103), RegionId(11), slot), Some(5));
    agent_details(&mut m, 103, &[("can_sabotage", 1, "subterfuge")], &[("subterfuge", 2)]);
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

/// recruit-gates: the two no-entry gates of `0x00B43CA0` / `0x00B77F90`. A unit the scripts restricted
/// (`0x00A27960`, the model's `UNIT_RESTRICTIONS` list) and a unit whose region unit resource (units #14,
/// `0x00AA1660`) the region's `RESOURCES_ARRAY` does not name get no entry; the queue step's removal reuses the
/// list, so a queued one is removed and refunded at the turn start.
#[test]
fn restricted_units_and_units_without_their_region_resource_are_not_recruitable() {
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    let listed = |m: &CampaignModel, k: &str| m.recruitable_units(r10).iter().any(|e| e.unit_key == k);
    assert!(listed(&m, "test_guard"));
    m.world.restricted_units.push("test_guard".into());
    assert!(!listed(&m, "test_guard") && listed(&m, "test_recruit"));
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: r10, unit_key: "test_guard".into(), target: None }),
        Err(CommandError::UnitNotAvailable("test_guard".into()))
    );
    m.world.restricted_units.clear();
    // A unit resource the region does not have: no entry; once the region names it, listed again.
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_guard").unwrap().unit_resource = Some("made_up_horses".into());
    m.rules = Arc::new(rules);
    assert!(!listed(&m, "test_guard") && listed(&m, "test_recruit"));
    m.world.regions.get_mut(&r10).unwrap().unit_resources = vec!["made_up_iron".into(), "made_up_horses".into()];
    assert!(listed(&m, "test_guard"));
    // Queued, then restricted: removed at the turn start and refunded like a cancel.
    m.apply(CampaignCommand::Recruit { region: r10, unit_key: "test_guard".into(), target: None }).unwrap();
    let after = m.world.factions[&A].treasury;
    let cost = m.world.regions[&r10].recruitment_queue[0].cost;
    m.world.restricted_units.push("test_guard".into());
    m.turn.humans = vec![A];
    m.start_campaign();
    assert!(m.world.regions[&r10].recruitment_queue.is_empty());
    assert_eq!(m.world.factions[&A].treasury, after + cost);
}

/// unitrestrict: the scripts' restricted units are the exe's list. An add appends even a listed unit
/// (`0x009C9700`), a remove erases the first match only (`0x00A1D3B0`), so a unit added twice stays out of the
/// recruit list and the recruit command until removed twice; a key the units table lacks is stored nowhere
/// (ORIGINAL BUG fixed: the exe appends a null record its save writer `0x009A1070` crashes on).
#[test]
fn a_unit_restricted_twice_stays_restricted_until_lifted_twice() {
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    let listed = |m: &CampaignModel| m.recruitable_units(r10).iter().any(|e| e.unit_key == "test_guard");
    assert!(m.add_restricted_unit("test_guard") && m.add_restricted_unit("test_guard"));
    assert!(!m.add_restricted_unit("made_up_no_unit") && !m.remove_restricted_unit("made_up_no_unit"));
    assert_eq!(m.world.restricted_units, ["test_guard", "test_guard"]);
    assert!(m.remove_restricted_unit("test_guard"));
    assert!(!listed(&m) && m.world.unit_restricted("test_guard"));
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: r10, unit_key: "test_guard".into(), target: None }),
        Err(CommandError::UnitNotAvailable("test_guard".into()))
    );
    assert!(m.remove_restricted_unit("test_guard") && m.remove_restricted_unit("test_guard"), "not listed: nothing to erase");
    assert!(m.world.restricted_units.is_empty() && listed(&m));
}

/// recruit-gates: `0x00A6AE40` reads the settlement container's `FORTIFICATION_SLOT` (+0x24) like the road
/// (+0x20), so the walls' building is a recruitable source, flagged when damaged.
#[test]
fn the_walls_building_is_a_recruitable_source() {
    use super::commands::ENTRY_DAMAGED;
    let mut m = test_model();
    let r10 = RegionId(10);
    let mut rules = (*m.rules).clone();
    let guard = rules.units["test_recruit"].clone();
    rules.units.insert("test_guard".into(), guard);
    rules.allow_unit_for_all("test_guard");
    rules.buildings.insert("test_walls".into(), super::rules::BuildingRules { chain: "test_walls_chain".into(), units_allowed: vec!["test_guard".into()], ..Default::default() });
    m.rules = Arc::new(rules);
    let guard = |m: &CampaignModel| m.recruitable_units(r10).into_iter().find(|e| e.unit_key == "test_guard").map(|e| e.flags);
    assert_eq!(guard(&m), None);
    m.world.regions.get_mut(&r10).unwrap().fortification = Some(BuildingRef { level_key: "test_walls".into(), health: 100 });
    assert_eq!(guard(&m), Some(0));
    m.world.regions.get_mut(&r10).unwrap().fortification.as_mut().unwrap().health = 40;
    assert_eq!(guard(&m), Some(ENTRY_DAMAGED));
}

/// recruit-gates: construction runs before the queue step (the original's round-end economy `0x008BC650` →
/// `0x00AB3DA0` steps it, and a completion rebuilds the recruitable cache, before the turn-start region update
/// `0x00AAE820` → `0x00B71FB0`). So a building finishing at the round end already allows its units at the next
/// turn start: an item of its unit is neither removed nor held back, and counts down.
#[test]
fn a_building_finished_this_turn_counts_for_the_queue_step() {
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    m.turn.humans = vec![A];
    m.start_campaign();
    let r = m.world.regions.get_mut(&r10).unwrap();
    r.slots[1].building = None;
    r.construction.push(ConstructionItem { slot: SlotRef::Slot(1), level_key: "test_depot".into(), turns_remaining: 1, cost: 0, kind: ConstructionKind::Build, active: true });
    r.recruitment_queue.push(RecruitmentItem { id: RecruitmentItemId(9001), unit_key: "test_guard".into(), turns_remaining: 3, cost: 400, target: None, target_lost: false });
    let ev = m.end_turn();
    let r = &m.world.regions[&r10];
    assert_eq!(r.slots[1].building.as_ref().map(|b| b.level_key.as_str()), Some("test_depot"));
    assert_eq!(r.recruitment_queue.iter().map(|i| (i.id.0, i.turns_remaining)).collect::<Vec<_>>(), vec![(9001, 2)]);
    let done = ev.iter().position(|e| *e == CampaignEvent::BuildingCompleted { region: r10, slot: SlotRef::Slot(1) }).unwrap();
    let start = ev.iter().position(|e| *e == CampaignEvent::FactionRoundStart { faction: A }).unwrap();
    assert!(done < start, "construction finishes in the round end, before the next round starts");
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
        let item = RecruitmentItem { id: RecruitmentItemId(id), unit_key: unit.into(), turns_remaining: 3, cost: 400, target: None, target_lost: false };
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

/// The settlement mask (`0x009C9EF0`): a spy (`subterfuge` >= 1) who has not acted this turn is
/// offered `embed` at another faction's settlement and anyone else `visit`; a settlement with a
/// research building (a building whose own effects give `research_points`, `0x00B28D20`) offers
/// an agent with `can_research` `research` at home and `steal_research` abroad instead of the
/// visit, unless the owner is the rebels. No room, or no `can_exchange` row, offers nothing.
#[test]
fn exchange_actions_at_a_settlement_follow_the_exes_mask() {
    use super::agents::{exchange_actions, AgentTarget, ExchangeActions};
    let at = |r: u32| AgentTarget::Settlement(RegionId(r));
    let visit = ExchangeActions { visit: true, ..Default::default() };
    let embed = ExchangeActions { embed: true, ..Default::default() };
    let research = ExchangeActions { research: true, ..Default::default() };
    let steal = ExchangeActions { steal_research: true, ..Default::default() };
    let (rake, own_g, foreign_g) = (CharacterId(103), CharacterId(104), CharacterId(105));
    let mut m = agents_model();
    // No research building effect yet: the rake embeds abroad and visits at home; a gentleman
    // (no subterfuge) visits anywhere.
    assert_eq!(exchange_actions(&m, rake, at(11)), embed);
    assert_eq!(exchange_actions(&m, rake, at(10)), visit);
    assert_eq!(exchange_actions(&m, own_g, at(10)), visit);
    assert_eq!(exchange_actions(&m, foreign_g, at(10)), visit);
    // An agent who has acted this turn (+0x4CC) only visits.
    m.world.agents_acted.insert(rake);
    assert_eq!(exchange_actions(&m, rake, at(11)), visit);
    m.world.agents_acted.clear();
    // Region 10's and 11's first slot hold `test_building_level`; give it research points.
    m.rules = test_model_with([map_basic("research_points", "research_points")]).rules;
    let mut rules = (*m.rules).clone();
    let rows = mapped(&rules, vec![("research_points", 10.0)]);
    rules.effects.insert_building_local("test_building_level".into(), rows);
    m.rules = Arc::new(rules);
    assert!(super::agents::has_research_building(&m, RegionId(10)));
    assert!(!super::agents::has_research_building(&m, RegionId(12)), "no building at all");
    // Without `can_research` nothing changes.
    assert_eq!(exchange_actions(&m, own_g, at(10)), visit);
    for g in [104, 105] {
        let d = m.world.character_details.get_mut(&CharacterId(g)).unwrap();
        d.abilities.push(("can_research".into(), 0, "research".into()));
    }
    assert_eq!(exchange_actions(&m, own_g, at(10)), research, "research replaces the visit at home");
    assert_eq!(exchange_actions(&m, foreign_g, at(10)), steal, "and is stolen abroad");
    // A rebel owner offers no research.
    m.world.factions.get_mut(&B).unwrap().key = String::new();
    assert_eq!(exchange_actions(&m, own_g, at(11)), visit);
    // No room in the settlement, or no `can_exchange` ability row: nothing at all.
    let mut rules = (*m.rules).clone();
    rules.limits.max_residence_characters = 0;
    m.rules = Arc::new(rules);
    assert_eq!(exchange_actions(&m, own_g, at(10)), ExchangeActions::default());
    let mut rules = (*m.rules).clone();
    rules.limits.max_residence_characters = 20;
    rules.no_exchange_ability = true;
    m.rules = Arc::new(rules);
    assert_eq!(exchange_actions(&m, own_g, at(10)), ExchangeActions::default());
}

/// The character mask (`0x009C9CA0`): a spy without a force is offered `embed` at another faction's
/// army; an own-faction target, or an agent without `subterfuge`, is offered nothing (the
/// own-faction attach of `0x009D0D00` is not ported).
#[test]
fn exchange_actions_at_a_character_embed_only_a_spy_in_a_foreign_army() {
    use super::agents::{exchange_actions, AgentTarget, ExchangeActions};
    let m = agents_model();
    let (rake, gent) = (CharacterId(103), CharacterId(104));
    let embed = ExchangeActions { embed: true, ..Default::default() };
    assert_eq!(exchange_actions(&m, rake, AgentTarget::Character(CharacterId(101))), embed);
    assert_eq!(exchange_actions(&m, rake, AgentTarget::Character(CharacterId(100))), ExchangeActions::default());
    assert_eq!(exchange_actions(&m, gent, AgentTarget::Character(CharacterId(101))), ExchangeActions::default());
    // A target who commands no force.
    assert_eq!(exchange_actions(&m, rake, AgentTarget::Character(CharacterId(105))), ExchangeActions::default());
}

/// `MoveIntoTarget` (`0x009EE150`): a spy sent at another faction's settlement spies on it, anyone
/// else sent at a settlement enters it as a resident; an own-faction character target (the
/// attach-to-force order `0x008FF7B0`) is refused.
#[test]
fn move_into_target_spies_or_enters() {
    use super::agents::AgentTarget;
    let mut m = agents_model();
    let (rake, gent) = (CharacterId(103), CharacterId(104));
    agent_details(&mut m, 103, &[("can_spy", 1, "subterfuge")], &[("subterfuge", 2)]);
    m.world.regions.get_mut(&RegionId(11)).unwrap().settlement.position = pos(0, 0);
    let ev = m.apply(CampaignCommand::MoveIntoTarget { agent: rake, target: AgentTarget::Settlement(RegionId(11)), research: false }).unwrap();
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::AgentActionResolved { agent, .. } if *agent == rake)), "{ev:?}");
    let r = RegionId(12);
    m.world.characters.get_mut(&gent).unwrap().position = pos(10, 0);
    let ev = m.apply(CampaignCommand::MoveIntoTarget { agent: gent, target: AgentTarget::Settlement(r), research: true }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEntersGarrison { character: gent, region: r }), "{ev:?}");
    assert_eq!(m.world.characters[&gent].garrisoned_in, Some(r));
    assert!(m.apply(CampaignCommand::MoveIntoTarget { agent: gent, target: AgentTarget::Character(CharacterId(100)), research: false }).is_err());
    assert!(m.apply(CampaignCommand::MoveIntoTarget { agent: gent, target: AgentTarget::Character(CharacterId(101)), research: false }).is_err());
}

#[test]
fn the_rogue_sabotage_army_target_is_the_residence_garrison() {
    use super::agents::rogue_sabotage_army_target;
    let mut m = agents_model();
    let (rake, force) = (CharacterId(103), ForceId(1001));
    // Not a resident anywhere: no army (`0x009FB9D0` finds no residence).
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(A)), None);
    // A resident of region 11, which B's force 1001 (commander 101) garrisons.
    m.world.characters.get_mut(&rake).unwrap().garrisoned_in = Some(RegionId(11));
    m.world.regions.get_mut(&RegionId(11)).unwrap().garrison = Some(force);
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(A)), Some(force));
    // The commander is the local player's: nothing (`0x009BF110` compared with commander +0x2A0).
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(B)), None);
    // A garrison without a commander: nothing.
    m.world.forces.get_mut(&force).unwrap().commander = None;
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(A)), None);
}

#[test]
fn the_rogue_sabotage_army_target_follows_his_own_land_army() {
    use super::agents::rogue_sabotage_army_target;
    let mut m = agents_model();
    let (rake, own) = (CharacterId(103), ForceId(2000));
    // He resides in region 11 (garrisoned by B's force 1001) and commands A's land army 2000.
    m.world.characters.get_mut(&rake).unwrap().garrisoned_in = Some(RegionId(11));
    m.world.regions.get_mut(&RegionId(11)).unwrap().garrison = Some(ForceId(1001));
    m.world.forces.insert(own, MilitaryForce { id: own, faction: A, commander: Some(rake), units: vec![unit(9)], is_navy: false });
    // His own land army's residence wins (`0x009FB9D0`: army +0x9C through `0x009D3A60`); it is in
    // none, so there is no army, and his own residence is not asked.
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(B)), None);
    // His army garrisons region 10: that residence's garrison is the target.
    m.world.regions.get_mut(&RegionId(10)).unwrap().garrison = Some(own);
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(B)), Some(own));
    // Its commander is the local player's: nothing.
    assert_eq!(rogue_sabotage_army_target(&m, rake, Some(A)), None);
}
