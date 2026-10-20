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
