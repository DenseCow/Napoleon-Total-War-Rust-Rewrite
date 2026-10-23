//! Campaign model tests: the treaty money in the running records (regular payments in categories 8 / 22,
//! protectorate tribute and patron income in 24 / 10; CAMPAIGN_FIDELITY.md "Running record").

use super::treaties::DiplomaticAction as D;
use super::*;

fn record(m: &CampaignModel, f: FactionId) -> EconomyRecord {
    *m.world.last_economy_record(f).unwrap()
}

fn act(m: &mut CampaignModel, a: FactionId, b: FactionId, action: D) {
    m.apply(CampaignCommand::Diplomacy { a, b, action }).unwrap();
}

fn treasuries(m: &CampaignModel) -> (i32, i32) {
    (m.world.factions[&A].treasury, m.world.factions[&B].treasury)
}

/// Ends faction turns until one round end has passed.
fn end_round(m: &mut CampaignModel) {
    let turns = m.calendar.turns_elapsed;
    while m.calendar.turns_elapsed == turns {
        m.end_turn();
    }
}

/// A regular payment books its amount once into the payer's category 22 and the payee's category 8
/// (`0x00B55010` → `0x00AE90A0`: a temporary item, its pushed copy, the temporary destroyed), and moves
/// no money by itself.
#[test]
fn a_regular_payment_is_booked_into_both_records() {
    let mut m = test_model();
    let before = treasuries(&m);
    act(&mut m, A, B, D::RegularPayment(100, 2));
    assert_eq!((record(&m, A)[22], record(&m, B)[8]), (100, 100));
    assert_eq!((record(&m, A)[8], record(&m, B)[22]), (0, 0));
    assert_eq!(treasuries(&m), before);
    act(&mut m, A, B, D::RegularPayment(50, 5));
    assert_eq!((record(&m, A)[22], record(&m, B)[8]), (150, 150));
}

/// The round end's settle pays a payment once per round (the twin model without it differs by exactly
/// the amount), and it is removed after its last round, taking its lines back. ORIGINAL BUG fix
/// (`0x0086A120`): the payee receives every payment the payer makes in either turn order (test model:
/// B, then A). With A paying, the exe's payer runs first and its update removes the item before B's
/// settle, so B would get one payment fewer.
#[test]
fn the_payee_receives_every_payment_in_either_turn_order() {
    for (payer, payee) in [(A, B), (B, A)] {
        let mut paying = test_model();
        let mut plain = test_model();
        act(&mut paying, payer, payee, D::RegularPayment(100, 2));
        for round in 1..=3 {
            end_round(&mut paying);
            end_round(&mut plain);
            let paid = 100 * round.min(2);
            let ((pa, pb), (qa, qb)) = (treasuries(&paying), treasuries(&plain));
            let (out, into) = if payer == A { (pa - qa, pb - qb) } else { (pb - qb, pa - qa) };
            assert_eq!((out, into), (-paid, paid), "payer {payer:?}, round {round}");
        }
        assert!(paying.world.relationships[&(payer, payee)].payments.is_empty());
        assert_eq!((record(&paying, payer)[22], record(&paying, payee)[8]), (0, 0));
    }
}

/// An ally joining a war (`0x00B268B0` → `0x00B26700` on its ally's behalf) drops its payments with the
/// enemy both ways, each item's destructor (`0x0086A120`) taking its lines back.
#[test]
fn an_ally_joining_a_war_takes_its_payments_back() {
    let mut m = test_model();
    // C needs land to be in the game.
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    act(&mut m, B, C, D::Alliance);
    act(&mut m, C, A, D::RegularPayment(100, 5));
    act(&mut m, A, C, D::RegularPayment(30, 5));
    assert_eq!((record(&m, C)[22], record(&m, A)[8]), (100, 100));
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(m.world.stance(C, A), Stance::War);
    assert!(m.world.relationships[&(C, A)].payments.is_empty() && m.world.relationships[&(A, C)].payments.is_empty());
    for f in [A, C] {
        let r = record(&m, f);
        assert_eq!((r[8], r[22]), (0, 0), "{f:?}");
    }
}

/// War ends both sides' payments (`0x00B27070` / `0x00B27250`), each item's destructor (`0x0086A120`)
/// taking its lines back.
#[test]
fn war_takes_the_payments_back() {
    let mut m = test_model();
    act(&mut m, A, B, D::RegularPayment(100, 5));
    act(&mut m, B, A, D::RegularPayment(30, 5));
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    for f in [A, B] {
        let r = record(&m, f);
        assert_eq!((r[8], r[22]), (0, 0), "{f:?}");
    }
}

/// The tribute is a fifth of categories 5 + 6 + 7 of the protectorate's running record, truncated
/// (`0x00BBCD00`; the other income, 11, does not count). The protectorate books it into its category 24
/// and the patron the same amount into its category 10, when the tie is made (`0x00B104E0` /
/// `0x00B10480`); ending the tie takes both back (`0x00B64C10` / `0x00B54530`).
#[test]
fn the_protectorate_tribute_is_booked_into_both_records() {
    let mut m = test_model();
    let mut running = [0; 25];
    (running[5], running[6], running[7], running[11]) = (1003, 0, 499, 400);
    m.world.economy_history.insert(B, vec![running]);
    act(&mut m, B, A, D::BecomeProtectorate);
    assert_eq!(m.world.relationships[&(B, A)].protectorate_tribute, 300);
    assert_eq!(m.world.relationships[&(A, B)].protectorate_income, 300);
    assert_eq!((record(&m, B)[24], record(&m, A)[10]), (300, 300));
    m.end_protectorate_tie(B, A).unwrap();
    assert_eq!((record(&m, B)[24], record(&m, A)[10]), (0, 0));
    assert_eq!(m.world.relationships[&(B, A)].protectorate_tribute, 0);
}

/// The per-turn update sets the tribute (the protectorate's records) and the income (the patron's) again
/// from the protectorate's running record (`0x00B29170`), booking only the change; the round start does
/// the same (`0x00B71EC0`).
#[test]
fn the_round_end_and_start_refresh_the_tribute() {
    let mut m = test_model();
    let mut running = [0; 25];
    running[5] = 500;
    m.world.economy_history.insert(B, vec![running]);
    act(&mut m, B, A, D::BecomeProtectorate);
    assert_eq!(record(&m, B)[24], 100);
    m.world.economy_history.get_mut(&B).unwrap().last_mut().unwrap()[5] = -1004;
    m.diplomacy_round_end(B, &mut Vec::new());
    assert_eq!((record(&m, B)[24], m.world.relationships[&(B, A)].protectorate_tribute), (-200, -200));
    assert_eq!(record(&m, A)[10], 100);
    m.diplomacy_round_end(A, &mut Vec::new());
    assert_eq!((record(&m, A)[10], m.world.relationships[&(A, B)].protectorate_income), (-200, -200));
    m.world.economy_history.get_mut(&B).unwrap().last_mut().unwrap()[5] = 1500;
    m.refresh_computed_factors();
    assert_eq!((record(&m, B)[24], record(&m, A)[10]), (300, 300));
}

/// A load books the treaty money again from the relationships (the record copy constructor `0x00AE9160`
/// rebooks every payment, tribute and income into the zero-filled categories).
#[test]
fn a_load_rebooks_the_treaty_money() {
    let mut m = test_model();
    let mut running = [0; 25];
    running[7] = 250;
    m.world.economy_history.insert(B, vec![running]);
    act(&mut m, A, B, D::RegularPayment(100, 5));
    act(&mut m, B, A, D::BecomeProtectorate);
    for f in [A, B] {
        m.world.economy_history.get_mut(&f).unwrap().last_mut().unwrap()[8..25].fill(0);
        economy::rebuild_running_record(&mut m, f);
    }
    let (a, b) = (record(&m, A), record(&m, B));
    assert_eq!((a[22], b[8], b[24], a[10]), (100, 100, 50, 50));
    assert_eq!((a[8], b[22], a[24], b[10]), (0, 0, 0, 0));
}

/// A faction leaving the game resets both records of each pair (`0x00B72900` / `0x00B2B920`), each line
/// taken back.
#[test]
fn leaving_the_game_takes_the_treaty_money_back() {
    let mut m = test_model();
    act(&mut m, A, B, D::RegularPayment(100, 5));
    act(&mut m, B, A, D::RegularPayment(40, 5));
    m.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
    let mut events = Vec::new();
    m.destroy_faction(B, &mut events);
    for f in [A, B] {
        let r = record(&m, f);
        assert_eq!((r[8], r[22]), (0, 0), "{f:?}");
    }
}
