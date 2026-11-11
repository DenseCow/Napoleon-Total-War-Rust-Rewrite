//! Campaign model tests: characters without a force resident in a settlement (the settlement
//! residence's list +0xC8, `SIEGEABLE_GARRISON_RESIDENCE` #14).

use super::*;

/// [`agents_model`] with the gentlemen limits set (positive 6, negative -8) and the room of a
/// residence set to `room`.
fn residence_model(room: u32) -> CampaignModel {
    let mut m = agents_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("gentleman_happiness_positive_limit".into(), 6.0);
    rules.variables.insert("gentleman_happiness_negative_limit".into(), -8.0);
    rules.limits.max_residence_characters = room;
    m.rules = Arc::new(rules);
    m
}

/// A gentleman ordered into his own settlement walks there and becomes a resident (the enter
/// `0x00B4E760`): `CharacterEntersGarrison` fires and his rank counts in the gentlemen factor. A
/// second order is a no-op; any move takes him out again (the leave `0x00B4EE70`) and the factor
/// drops back to 0.
#[test]
fn a_gentleman_enters_and_leaves_a_settlement() {
    use super::economy::gentlemen_factor;
    let mut m = residence_model(20);
    let (g, r) = (CharacterId(104), RegionId(12));
    m.world.characters.get_mut(&g).unwrap().position = pos(10, 0);
    assert_eq!(gentlemen_factor(&m, &m.world.regions[&r]), 0.0);
    let ev = m.apply(CampaignCommand::AgentEnterSettlement { agent: g, region: r }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEntersGarrison { character: g, region: r }));
    let ch = &m.world.characters[&g];
    assert_eq!((ch.garrisoned_in, ch.position), (Some(r), m.world.regions[&r].settlement.position));
    assert_eq!(m.settlement_residents(r), vec![g]);
    // Rank 2 (research 2): +2 for the owner A.
    assert_eq!(gentlemen_factor(&m, &m.world.regions[&r]), 2.0);
    assert_eq!(m.apply(CampaignCommand::AgentEnterSettlement { agent: g, region: r }), Ok(Vec::new()));
    m.apply(CampaignCommand::MoveCharacter { character: g, to: pos(13, 0) }).unwrap();
    assert_eq!(m.world.characters[&g].garrisoned_in, None);
    assert!(m.settlement_residents(r).is_empty());
    assert_eq!(gentlemen_factor(&m, &m.world.regions[&r]), 0.0);
}

/// The room (`0x00B165B0`): without a garrison, an agent enters while the settlement holds fewer
/// residents than [`crate::limits::GameLimits::max_residence_characters`]; a full settlement leaves
/// him standing on it, outside. A type that does not count (`agents` #4 false) enters his own
/// faction's settlement whatever the count.
#[test]
fn a_full_settlement_keeps_an_agent_outside() {
    let mut m = residence_model(2);
    let r = RegionId(12);
    for id in [106, 107] {
        let mut c = character(id, A, CharacterKind::Rake, 10);
        (c.position, c.garrisoned_in) = (m.world.regions[&r].settlement.position, Some(r));
        m.world.characters.insert(c.id, c);
    }
    let g = CharacterId(104);
    m.world.characters.get_mut(&g).unwrap().position = pos(10, 0);
    assert!(!m.may_reside(g, r));
    let ev = m.apply(CampaignCommand::AgentEnterSettlement { agent: g, region: r }).unwrap();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CharacterEntersGarrison { .. })));
    let ch = &m.world.characters[&g];
    assert_eq!((ch.garrisoned_in, ch.position), (None, m.world.regions[&r].settlement.position));
    // A resident already inside does not count against himself.
    assert!(m.may_reside(CharacterId(106), r));
    let mut rules = (*m.rules).clone();
    rules.residence_exempt_agents.insert("gentleman".into());
    m.rules = Arc::new(rules);
    assert!(m.may_reside(g, r));
    m.apply(CampaignCommand::AgentEnterSettlement { agent: g, region: r }).unwrap();
    assert_eq!(m.settlement_residents(r), vec![CharacterId(104), CharacterId(106), CharacterId(107)]);
}

/// With a garrison army inside, the owner's agents are counted against that army's accompanying
/// characters (`0x008B2620`; none in the model), so they enter a settlement whose residents fill
/// the room; another faction's agent is still counted against the residents. Exempt types do not
/// help a foreign agent. A commander is refused: his force enters with `EnterSettlement`.
#[test]
fn a_garrison_takes_its_own_agents_in() {
    let mut m = residence_model(1);
    let r = RegionId(12);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(r);
    m.world.regions.get_mut(&r).unwrap().garrison = Some(ForceId(1000));
    let mut c = character(106, A, CharacterKind::Rake, 10);
    c.garrisoned_in = Some(r);
    m.world.characters.insert(c.id, c);
    assert_eq!(m.settlement_residents(r), vec![CharacterId(106)]);
    assert!(m.may_reside(CharacterId(104), r));
    let mut rules = (*m.rules).clone();
    rules.residence_exempt_agents.insert("gentleman".into());
    m.rules = Arc::new(rules);
    assert!(!m.may_reside(CharacterId(105), r));
    m.world.characters.get_mut(&CharacterId(104)).unwrap().position = pos(10, 0);
    m.apply(CampaignCommand::AgentEnterSettlement { agent: CharacterId(104), region: r }).unwrap();
    assert_eq!(m.world.characters[&CharacterId(104)].garrisoned_in, Some(r));
    assert!(matches!(
        m.apply(CampaignCommand::AgentEnterSettlement { agent: CharacterId(100), region: r }),
        Err(CommandError::Unsupported(_))
    ));
}

/// A capture (`0x00B44920`, after the owner change): a resident of a spy type whose faction the new
/// owner is at war with and whom it knows is executed, his faction told first
/// (`your_spy_detected_execute`), then the new owner (`spy_detected_execute`); a spy it does not
/// know, a spy of a faction at peace with it and every other resident stay inside.
#[test]
fn a_capture_executes_the_known_hostile_spies_inside() {
    let mut m = residence_model(20);
    let r = RegionId(12);
    let inside = m.world.regions[&r].settlement.position;
    for id in [103, 104] {
        let ch = m.world.characters.get_mut(&CharacterId(id)).unwrap();
        (ch.position, ch.garrisoned_in) = (inside, Some(r));
    }
    let mut c_rake = character(106, C, CharacterKind::Rake, 10);
    (c_rake.position, c_rake.garrisoned_in) = (inside, Some(r));
    m.world.characters.insert(c_rake.id, c_rake);
    agent_details(&mut m, 106, &[], &[("subterfuge", 2)]);
    set_details(&mut m, A, Some(10), false);
    set_details(&mut m, B, Some(11), false);
    m.world.set_stance(B, A, Stance::War).unwrap();
    m.world.set_stance(B, C, Stance::Neutral).unwrap();
    m.expose(B, CharacterId(106));

    // B does not know A's rake (subterfuge 2, never exposed): nobody is executed.
    let mut ev = Vec::new();
    m.occupy(r, B, None, &mut ev);
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::Message { message: CampaignMessage::SpyDetectedExecute { .. }, .. })));
    assert_eq!(m.settlement_residents(r), vec![CharacterId(103), CharacterId(104), CharacterId(106)]);

    // Back to A; now B has exposed him: B's next capture executes him, and only him.
    m.occupy(r, A, None, &mut Vec::new());
    m.expose(B, CharacterId(103));
    let mut ev = Vec::new();
    m.occupy(r, B, None, &mut ev);
    let messages: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::Message { recipient, message } if matches!(message, CampaignMessage::SpyDetectedExecute { .. } | CampaignMessage::YourSpyDetectedExecute { .. }) => {
                Some((*recipient, message.type_name()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(messages, vec![(A, "your_spy_detected_execute"), (B, "spy_detected_execute")]);
    assert!(ev.iter().any(|e| matches!(
        e,
        CampaignEvent::Message { recipient: B, message: CampaignMessage::SpyDetectedExecute { character: CharacterId(103), at, .. } } if *at == inside
    )));
    assert!(!m.world.characters.contains_key(&CharacterId(103)));
    assert_eq!(m.settlement_residents(r), vec![CharacterId(104), CharacterId(106)]);
}

/// A General hired into a garrison army takes its command inside (`0x00A164C0`: the unit added by
/// `0x008CFBE0`, the commander picked again by `0x008D2FA0`), and the old commander rides on with his
/// unit, inside through the army and not one of the settlement's residents: the hire reaches no
/// residence list (`0x008CFA70` / `0x00B4EE70`), so he takes no room. A rider still marked inside
/// (what the hire left before) is no resident either ([`World::residence_of`]).
#[test]
fn a_general_hired_into_a_garrison_leaves_no_resident() {
    let mut m = test_model();
    let r = RegionId(10);
    m.world.faction_details.entry(A).or_default().capital = Some(r);
    let mut rules = (*m.rules).clone();
    rules.general_units.insert(String::new(), "test_unit".into());
    m.rules = Arc::new(rules);
    let mut col = character(107, A, CharacterKind::Colonel, 25);
    (col.position, col.garrisoned_in) = (m.world.regions[&r].settlement.position, Some(r));
    m.world.characters.insert(col.id, col);
    let mut u = unit(7);
    u.character = Some(CharacterId(107));
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![u], is_navy: false });
    m.world.regions.get_mut(&r).unwrap().garrison = Some(ForceId(1002));
    let elapsed = m.calendar.turns_elapsed;
    m.world.faction_details.get_mut(&A).unwrap().general_pool = (Vec::new(), elapsed);
    m.world.faction_details.get_mut(&A).unwrap().admiral_pool = (Vec::new(), elapsed + 100);
    let c = m.pool_tick(A)[0];
    m.turn.humans = vec![A];
    m.start_campaign();
    m.apply(CampaignCommand::HireGeneral { character: c, into: Some(ForceId(1002)) }).unwrap();
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(c));
    assert_eq!(m.world.characters[&c].garrisoned_in, Some(r));
    assert_eq!(m.world.characters[&CharacterId(107)].garrisoned_in, None);
    assert_eq!((m.world.residence_of(c), m.world.residence_of(CharacterId(107))), (None, None));
    assert!(!m.settlement_residents(r).contains(&CharacterId(107)));
    m.world.characters.get_mut(&CharacterId(107)).unwrap().garrisoned_in = Some(r);
    assert_eq!(m.world.residence_of(CharacterId(107)), None);
    assert!(!m.settlement_residents(r).contains(&CharacterId(107)));
    assert!(!m.world.settlement_residence_index().contains_key(&CharacterId(107)));
}

/// After a successful action on a character inside a settlement the agent enters it
/// (`0x00915150` with its enter flag, from `0x0094ACB0` / `0x0094DA30`): a resident's settlement,
/// or the one his army garrisons (`0x009FB9A0`). The room counts the target, who is still inside
/// then, and an agent already inside fires nothing. A failure or critical failure enters nothing.
#[test]
fn a_successful_agent_enters_his_targets_settlement() {
    use super::agents::Outcome;
    let mut m = residence_model(1);
    let r = RegionId(12);
    let inside = m.world.regions[&r].settlement.position;
    let (agent, target) = (CharacterId(103), CharacterId(105));
    let t = m.world.characters.get_mut(&target).unwrap();
    (t.position, t.garrisoned_in) = (inside, Some(r));
    m.world.characters.get_mut(&agent).unwrap().position = inside;
    // Room 1, the target fills it.
    assert!(m.residence_after_action(agent, target, Outcome::Success).is_empty());
    assert_eq!(m.world.characters[&agent].garrisoned_in, None);
    let mut rules = (*m.rules).clone();
    rules.limits.max_residence_characters = 2;
    m.rules = Arc::new(rules);
    for outcome in [Outcome::Failure, Outcome::CriticalFailure] {
        assert!(m.residence_after_action(agent, target, outcome).is_empty());
        assert_eq!(m.world.characters[&agent].garrisoned_in, None);
    }
    assert_eq!(m.residence_after_action(agent, target, Outcome::CriticalSuccess), vec![CampaignEvent::CharacterEntersGarrison { character: agent, region: r }]);
    assert_eq!(m.settlement_residents(r), vec![agent, target]);
    assert!(m.residence_after_action(agent, target, Outcome::Success).is_empty());

    // A commander inside through his garrison army: B's colonel 101 in B's region 11.
    let r11 = RegionId(11);
    m.world.regions.get_mut(&r11).unwrap().garrison = Some(ForceId(1001));
    m.world.characters.get_mut(&CharacterId(101)).unwrap().garrisoned_in = Some(r11);
    assert_eq!(m.character_residence(CharacterId(101)), Some(r11));
    assert_eq!(m.residence_after_action(agent, CharacterId(101), Outcome::Success), vec![CampaignEvent::CharacterEntersGarrison { character: agent, region: r11 }]);
    let a = &m.world.characters[&agent];
    assert_eq!((a.garrisoned_in, a.position), (Some(r11), m.world.regions[&r11].settlement.position));
    // A commander outside any settlement: nothing to enter.
    m.world.regions.get_mut(&r11).unwrap().garrison = None;
    assert_eq!(m.character_residence(CharacterId(101)), None);
}

/// After a failed action an agent inside a settlement that is not under siege leaves it to the
/// free spot near it (`0x00915150` with its leave flag, `0x00B3B460` → `0x00B39FE0`; without a
/// polygon map the settlement's own position); under siege he stays inside. Success and critical
/// failure leave him where he is.
#[test]
fn a_failed_agent_leaves_his_settlement_unless_besieged() {
    use super::agents::Outcome;
    let mut m = residence_model(20);
    let r = RegionId(12);
    let inside = m.world.regions[&r].settlement.position;
    let agent = CharacterId(103);
    let a = m.world.characters.get_mut(&agent).unwrap();
    (a.position, a.garrisoned_in) = (inside, Some(r));
    m.world.regions.get_mut(&r).unwrap().siege.besiegers = vec![ForceId(1001)];
    assert!(m.residence_after_action(agent, CharacterId(105), Outcome::Failure).is_empty());
    assert_eq!(m.world.residence_of(agent), Some(r));
    m.world.regions.get_mut(&r).unwrap().siege.besiegers.clear();
    for outcome in [Outcome::Success, Outcome::CriticalFailure] {
        m.residence_after_action(agent, CharacterId(105), outcome);
        assert_eq!(m.world.residence_of(agent), Some(r));
    }
    assert!(m.residence_after_action(agent, CharacterId(105), Outcome::Failure).is_empty());
    let a = &m.world.characters[&agent];
    assert_eq!((a.garrisoned_in, a.position), (None, inside));
    assert!(m.settlement_residents(r).is_empty());
}

/// The assassination's order (`0x0094ACB0`): the agent enters the target's settlement
/// (`CharacterEntersGarrison`) before `AssassinationAttemptSuccess`, and the target falls after.
#[test]
fn an_assassin_enters_before_the_success_event() {
    let mut m = residence_model(20);
    let r = RegionId(12);
    let inside = m.world.regions[&r].settlement.position;
    let (agent, target) = (CharacterId(103), CharacterId(105));
    let t = m.world.characters.get_mut(&target).unwrap();
    (t.position, t.garrisoned_in) = (inside, Some(r));
    m.world.characters.get_mut(&agent).unwrap().position = inside;
    m.turn.humans = vec![A];
    m.world.force_success_for_human.0 = true;
    let ev = m.apply(CampaignCommand::Assassinate { agent, target }).unwrap();
    let enter = ev.iter().position(|e| *e == CampaignEvent::CharacterEntersGarrison { character: agent, region: r });
    let success = ev.iter().position(|e| matches!(e, CampaignEvent::AssassinationAttemptSuccess { character, .. } if *character == agent));
    assert!(enter.is_some() && enter < success, "{ev:?}");
    assert!(!m.world.characters.contains_key(&target));
    assert_eq!(m.settlement_residents(r), vec![agent]);
}
