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

/// A (human) and B (AI, holding only its capital 11) at war; B's army is off the map, A's stands in
/// 11 (`garrisoned_in` without being the garrison, so it is A's strength there) when `in_capital`.
fn ai_war_model(in_capital: bool) -> CampaignModel {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(1000, 1000);
    if in_capital {
        m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(11));
    }
    m
}

/// The AI's peace attitude (`0x00D63260`): 0 when not at war; at war with no reason for peace,
/// max(−1, −0.8); with the enemy in its lone capital and none of its own strength there
/// (`0x00D593F0`, no neighbour: q ≥ 7 → 0.9 > 0, so no protectorate or ally holds it back) 1; a long
/// war going badly (momentum below −10 after more than 1 turn) 1 too.
#[test]
fn the_ai_peace_attitude_follows_the_war() {
    let m = test_model();
    assert_eq!(m.peace_attitude(B, A, false), 0.0);
    let m = ai_war_model(false);
    assert_eq!(m.peace_attitude(B, A, false), -0.8);
    assert_eq!(m.peace_attitude(B, A, true), -0.8);
    let m = ai_war_model(true);
    assert_eq!(m.peace_attitude(B, A, true), 1.0);
    let mut m = ai_war_model(false);
    let r = m.world.relationships.get_mut(&(B, A)).unwrap();
    (r.war_turns, r.war_momentum) = (2, -11);
    assert_eq!(m.peace_attitude(B, A, true), 1.0);
    m.world.relationships.get_mut(&(B, A)).unwrap().war_momentum = -10;
    assert_eq!(m.peace_attitude(B, A, true), -0.8);
}

/// The Peninsular campaign's refusal (`0x00D63260`): towards a human whose attitude total is at
/// most −200 the AI's peace attitude is −1, even with the enemy in its capital; −199 is not refused.
#[test]
fn the_peninsular_ai_refuses_peace_with_a_hated_human() {
    let mut m = ai_war_model(true);
    let mut rules = (*m.rules).clone();
    rules.features.ai_peace_refusal_attitude = Some(-200);
    m.rules = Arc::new(rules);
    let r = m.world.relationships.get_mut(&(B, A)).unwrap();
    r.attitudes = vec![details::AttitudeFactor::default(); 24];
    r.attitudes[7].value = -200;
    assert_eq!(m.peace_attitude(B, A, true), -1.0);
    m.world.relationships.get_mut(&(B, A)).unwrap().attitudes[7].value = -199;
    assert_eq!(m.peace_attitude(B, A, true), 1.0);
}

/// An AI recipient answers a peace deal (record 9): evaluated, not left unanswered; declined when
/// its peace option towards the proposer forbids accepting (1, `0x00B27FE0`).
#[test]
fn an_ai_answers_a_peace_deal() {
    let mut m = ai_war_model(true);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    assert!(m.ai_accepts_deal().is_some());
    let peace = negotiation::NegotiationAction::Peace.option();
    m.world.relationships.get_mut(&(B, A)).unwrap().diplomacy_options[peace] = 1;
    assert_eq!(m.ai_accepts_deal(), Some(false));
}

/// The trade route seam answering `codes` ((from, to, code); any other pair 0).
fn with_routes(m: &mut CampaignModel, codes: Vec<(FactionId, FactionId, u8)>) {
    let mut rules = (*m.rules).clone();
    let maker: crate::seam::Maker<super::seams::TradeRouteCheck> = Arc::new(move |_| {
        let codes = codes.clone();
        Arc::new(move |_, a, b| codes.iter().find(|(x, y, _)| (*x, *y) == (a, b)).map_or(0, |c| c.2))
    });
    rules.seams.trade_route_check.apply("test_mod", &maker);
    m.rules = Arc::new(rules);
}

/// The open negotiation's possible flags, record by record.
fn possible(m: &CampaignModel) -> Vec<bool> {
    m.negotiations.current.as_ref().unwrap().actions.iter().map(|a| a.possible).collect()
}

/// The records the peace record's side effects touch: trade, access, alliance, protector, break
/// trade, break alliance.
const LINKED: [usize; 6] = [0, 1, 3, 8, 12, 13];

/// Taking the peace deal (`0x00C4ADE0`, CONFIRMED disassembly) at war: the peace record becomes
/// impossible; access and alliance possible; trade only with a route both ways (`0x00BA42E0`);
/// break trade when either side trades with a third faction (`0x00C3E410`), break alliance when
/// either is allied with one (`0x00C3D5A0`); protector stays off (`0x00C1A770` needs peace). Clearing
/// it (`0x00C5CC40`) makes the peace record possible again and the six impossible. The panel lists
/// what the flags say.
#[test]
fn taking_the_peace_deal_opens_the_linked_records_and_clearing_closes_them() {
    use super::negotiation::NegotiationAction as N;
    let mut m = war_model();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let start = possible(&m);
    assert!(LINKED.iter().all(|&i| !start[i]), "the constructor leaves them impossible at war");
    assert!(start[9], "peace is possible at war");
    let listed = |m: &CampaignModel, action| m.possible_actions(A, A, B).offers_and_demands.iter().any(|a| a.action == action);
    assert!(listed(&m, N::Peace) && !listed(&m, N::Access));

    // No route, no third party: only access and alliance open.
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    let p = possible(&m);
    assert_eq!(LINKED.map(|i| p[i]), [false, true, true, false, false, false]);
    assert!(!p[9]);
    assert!(!listed(&m, N::Peace) && listed(&m, N::Access), "the panel reads the negotiation's flags");
    // The other records are not touched.
    assert_eq!([2, 4, 5, 6, 7, 10, 11].map(|i| p[i]), [2, 4, 5, 6, 7, 10, 11].map(|i| start[i]));

    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    assert_eq!(possible(&m), start, "the clear restores the constructor's flags at war");

    // A route one way only: trade stays off; both ways: on. B trades with C, A is allied with C.
    with_routes(&mut m, vec![(A, B, 1)]);
    m.relationship_mut(B, C).trade_agreement = true;
    m.world.set_stance(A, C, Stance::Allied).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    assert_eq!(LINKED.map(|i| possible(&m)[i]), [false, true, true, false, true, true]);
    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    with_routes(&mut m, vec![(A, B, 1), (B, A, 1)]);
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    assert_eq!(LINKED.map(|i| possible(&m)[i]), [true, true, true, false, true, true]);
    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    assert_eq!(possible(&m), start);
}

/// The take never makes a record impossible and the clear runs whatever the record holds
/// (`0x00933BF0`): with C (AI, one region) at peace with A, the protector record is possible from
/// the constructor; a clear with no peace held closes it (and opens the peace record), a take opens
/// it again (`0x00C1A770` passes at peace).
#[test]
fn the_peace_records_clear_and_take_reach_the_protector_record() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.regions.insert(RegionId(13), region(13, C, 1000, 0));
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: C }).unwrap();
    assert!(possible(&m)[8] && !possible(&m)[9]);
    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    assert!(!possible(&m)[8] && possible(&m)[9]);
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    assert!(possible(&m)[8] && !possible(&m)[9]);
}

/// `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (`0x00C1D140`) clears only the records holding items: without a
/// peace item the flags stay; with one the peace clear runs. Accepting a deal (`0x00C43520` at
/// `0x00C11650`) makes every record impossible.
#[test]
fn clearing_or_accepting_the_deal_sets_the_peace_records_flags() {
    let mut m = war_model();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let start = possible(&m);
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: true }).unwrap();
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    let taken = possible(&m);
    m.apply(CampaignCommand::ProposePayment { clear: false, amount: 100, turns: 1 }).unwrap();
    m.apply(CampaignCommand::ProposePayment { clear: true, amount: 0, turns: 1 }).unwrap();
    assert_eq!(possible(&m), taken, "another record's clear leaves them");
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert_eq!(possible(&m), start);
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert_eq!(possible(&m), start, "no peace item: the peace clear does not run");
    m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert!(possible(&m).iter().all(|&p| !p));
    assert!(m.possible_actions(A, A, B).offers_and_demands.is_empty());
}

/// The join-war record's clear (vtable `0x0138202C` +0x50, `0x00C5CA30`, CONFIRMED disassembly)
/// empties its lists and makes it possible (`0x00C5CA40`): after an accept has zeroed every flag,
/// `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (`0x00C1D140`) reopens record 11 only when it holds factions
/// (has-items `0x00C125E0`), and `CCQ_DIPLOMACY_PROPOSE_JOIN_WAR` with the clear flag (`0x009339AB`)
/// reopens it whatever it holds. The peace item the accept leaves in the deal is cleared too
/// (record 9 reopened, `0x00C5CC40`); no other record changes.
#[test]
fn the_join_war_records_clear_makes_it_possible() {
    let accepted = || {
        let mut m = war_model();
        m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
        m.apply(CampaignCommand::ProposePeace { clear: false }).unwrap();
        m.apply(CampaignCommand::AcceptDeal).unwrap();
        assert!(possible(&m).iter().all(|&p| !p));
        m
    };
    let open = |m: &CampaignModel| possible(m).iter().enumerate().filter(|&(_, &p)| p).map(|(i, _)| i).collect::<Vec<_>>();

    let mut m = accepted();
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert_eq!(open(&m), [9], "no join-war item: its clear does not run");
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![] }).unwrap();
    assert_eq!(open(&m), [9], "the take writes no flag");
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert_eq!(open(&m), [9, 11]);
    assert!(m.negotiations.current.as_ref().unwrap().join_war.is_empty());

    let mut m = accepted();
    m.apply(CampaignCommand::ProposeJoinWar { clear: true, demanded: vec![], offered: vec![] }).unwrap();
    assert_eq!(open(&m), [11], "the clear command runs with no item held");
}
