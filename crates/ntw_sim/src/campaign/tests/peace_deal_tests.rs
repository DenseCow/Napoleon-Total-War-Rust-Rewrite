//! Campaign model tests: a deal's peace record (record 9, negotiation `+0x184`).

use super::*;

/// `force`'s commander `ch` garrisons `region`'s settlement.
fn garrison(m: &mut CampaignModel, force: u32, ch: i32, region: u32) {
    m.world.regions.get_mut(&RegionId(region)).unwrap().garrison = Some(ForceId(force));
    m.world.characters.get_mut(&CharacterId(ch)).unwrap().garrisoned_in = Some(RegionId(region));
}

/// A (human) and B (human) at war; A's army (force 1000, general 100) holds B's settlement 11 and
/// B's (force 1001, colonel 101) A's settlement 10. B also holds region 13.
fn war_model() -> CampaignModel {
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    m.world.regions.insert(RegionId(13), region(13, B, 1000, 0));
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    garrison(&mut m, 1000, 100, 11);
    garrison(&mut m, 1001, 101, 10);
    m
}

/// `CCQ_DIPLOMACY_PROPOSE_PEACE` (`0x00933B90`): the flag sets the record (`0x00C4ADE0`) or clears it
/// (`0x00C5CC40`), each a change of the deal; `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (`0x00C1D140`) clears it
/// too. No negotiation, no record.
#[test]
fn the_peace_record_is_proposed_and_cleared() {
    let mut m = war_model();
    assert_eq!(m.apply(CampaignCommand::ProposePeace { clear: false }), Err(CommandError::NoNegotiation));
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let peace = |m: &CampaignModel| m.negotiations.current.as_ref().unwrap().peace;
    assert!(!peace(&m));
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    assert!(peace(&m));
    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    assert!(!peace(&m));
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert!(!peace(&m));
}

/// Accepting a peace deal (`0x00C114B0`): the peace is made first (`0x00C18B60` → `0x00B262C0`, the
/// stance change the first event), then each side's forces leave the other's regions (`0x00C11519`..
/// `0x00C115A2` → `0x008D9DC0`): A's army leaves B's settlement 11 for A's nearest region, 10; B's
/// leaves A's 10 for its own 11 (the nearest of 11 and 13). Applied once.
#[test]
fn a_peace_deal_moves_both_sides_out_of_each_others_regions() {
    let mut m = war_model();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    let ev = m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(ev.first(), Some(&CampaignEvent::StanceChanged { a: A, b: B, stance: Stance::Neutral }));
    assert_eq!(m.world.stance(A, B), Stance::Neutral);
    assert_eq!((m.world.regions[&RegionId(10)].garrison, m.world.regions[&RegionId(11)].garrison), (None, None));
    for (ch, to) in [(100, 10), (101, 11)] {
        let c = &m.world.characters[&CharacterId(ch)];
        assert_eq!((c.garrisoned_in, c.position), (None, pos(to, 0)), "character {ch}");
    }
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Ok(Vec::new()));
}

/// A deal without a peace item runs no peace pass: at peace already, each side's army stays in the
/// other's settlement after a payment deal (though the rule would put it out, as
/// `CCQ_MOVE_ARMIES_OUT_OF_REGIONS` shows). A peace item between factions not at war refuses the
/// whole deal before anything is applied.
#[test]
fn a_deal_without_peace_moves_no_one() {
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    garrison(&mut m, 1000, 100, 11);
    garrison(&mut m, 1001, 101, 10);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposePayment { clear: false, amount: 100, turns: 1 }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    let treasury = m.world.factions[&B].treasury;
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::NotAtWar(A, B)));
    assert_eq!(m.world.factions[&B].treasury, treasury);
    assert!(!m.negotiations.current.as_ref().unwrap().applied);
    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.factions[&B].treasury, treasury - 100, "the payment was applied");
    assert_eq!((m.world.regions[&RegionId(10)].garrison, m.world.regions[&RegionId(11)].garrison), (Some(ForceId(1001)), Some(ForceId(1000))));
    m.apply(CampaignCommand::MoveArmiesOutOfRegions { a: A, b: B }).unwrap();
    assert_eq!((m.world.regions[&RegionId(10)].garrison, m.world.regions[&RegionId(11)].garrison), (None, None));
}

/// A peace deal with a region exchange: the region record is applied before the peace pass, which
/// reads each side's region list after the transfer. A demands B's region 11, which its army holds:
/// 11 is A's by the pass, so the army stays in its settlement. B's army leaves A's regions, 11 among
/// them now, for B's remaining region 13. A human receiving a region in a deal with a peace item does
/// not count it (`0x00C18BF0`).
#[test]
fn a_peace_deals_pass_reads_the_regions_after_the_exchange() {
    let mut m = war_model();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert_eq!(m.world.regions[&RegionId(11)].garrison, Some(ForceId(1000)));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, Some(RegionId(11)));
    assert_eq!(m.world.regions[&RegionId(10)].garrison, None);
    let c = &m.world.characters[&CharacterId(101)];
    assert_eq!((c.garrisoned_in, c.position), (None, pos(13, 0)));
    assert_eq!(m.world.deal_regions_received.get(&A), None);
}

/// A peace item between factions at peace: the exe never lets one reach an accept (the record is
/// possible only at war, `0x00BF3CD0` → `0x00C1A730`; the panel, scripts and AI offer it only then;
/// CAMPAIGN_FIDELITY.md "The deal's peace record"), so ours refuses the deal: no stance change, no
/// event, nothing applied, the item kept.
#[test]
fn a_peace_item_between_factions_at_peace_changes_nothing() {
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    let stance = m.world.stance(A, B);
    assert_ne!(stance, Stance::War);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::NotAtWar(A, B)));
    assert_eq!(m.world.stance(A, B), stance);
    let n = m.negotiations.current.as_ref().unwrap();
    assert!(n.peace && !n.applied);
}
