//! The original's treasury money rules (CAMPAIGN_FIDELITY.md §Construction cost, §Recruitment, §Treasury
//! changes): the affordability tests and the one path every treasury change takes. Every write to a faction's
//! treasury goes through [`pay`] (the exe's charge `0x00BAF500`), [`credit`] (its credit `0x00BB3810`),
//! [`settle_round`] (the round's settle `0x00BABE30`) or [`script_credit`] (the script's `treasury_mod`),
//! all 32-bit wrapping arithmetic as the exe's; the bankruptcy reset to 0 is the one plain store
//! (`economy::settle_round`). CONFIRMED callers (static trace, 2026-10-10):
//!
//! - charge: construction and repair (category 0), recruitment (2, `0x00B58DD0`), a hired pool candidate
//!   (2, `0x00A1B8F0`, a human faction only), a promotion in the field (`0x008E1C20` → slot +0x44 →
//!   `0x00BAF500`, every faction), a one-off treaty payment's payer (3, `0x00C18A70`) and a state gift's
//!   giver (3, `0x00C4B440`);
//! - credit: a cancelled construction, repair or recruitment item (3), a one-off treaty payment's payee
//!   (1, `0x00C18A70`), a capture option's money (0, `0x00B541E0` → `0x00BB3810` at `0x00B5431C`), the
//!   script's `treasury_mod` (3, `0x0097BAD0`) and the console (4).
//!
//! Every charge and credit passes the faction economics' per-category converter (spending `+0x42C`,
//! `0x00BA8CD0`; income `+0x3F8`, `0x00BA8CE0`; vtable +4). CONFIRMED identity for every category of a game
//! started in the model: a new faction economics (`0x00B96100`) sets all 13 income and 12 spending converters
//! to `0x01459050`, whose +4 (`0x004A23F0`) returns its argument; the only other converter, `0x0145904C`
//! (+4 `0x0044C230`, returns 0), is set only by the loader `0x00B95B90` (from a saved type byte, and for
//! income category 12 of a version-1 save), and the saver `0x00BC4F20` writes back the type it holds.

/// The construction card's "too dear" test, inverted (`0x00B43300`, flag 1, JBE): cost ≤ treasury compared as
/// unsigned 32-bit values. With a treasury below 0 every cost ≥ 0 passes; a cost below 0 passes only when the
/// treasury is below 0 too and not below it (treasury −5, cost −300: passes; treasury 1000, cost −3: fails).
pub fn construction_affordable(treasury: i32, cost: i32) -> bool {
    cost as u32 <= treasury as u32
}

/// May the construct command pay `cost`? Its option list (`0x00B43880` with flags 1, 1) drops the options
/// [`construction_affordable`] fails, then `0x00B13DD0` refuses when treasury < cost compared signed (JL).
pub fn can_pay_construction(treasury: i32, cost: i32) -> bool {
    construction_affordable(treasury, cost) && treasury >= cost
}

/// May a repair be paid (`0x00B16430`: the panel's `can_afford_repair` via `0x009C8190`, and the AI's repair
/// `0x00AA4910`)? The repair cost (`0x00B66410`, capped at the treasury for an AI faction, so an AI always
/// passes) ≤ treasury, signed. The repair command itself (`CCQ_BUILDING_REPAIR` handler `0x00931DA0` → `0x00B66260`) tests nothing.
pub fn can_pay_repair(treasury: i32, cost: i32) -> bool {
    cost <= treasury
}

/// The recruitable entry's "too dear" flag, inverted (`0x00B69BA0`, flag 2, JBE): the same unsigned test as
/// [`construction_affordable`]. The recruit command (`0x00B58DD0`) refuses any flagged entry and has no signed
/// test of its own, so a faction in debt may recruit.
pub fn recruitment_affordable(treasury: i32, cost: i32) -> bool {
    construction_affordable(treasury, cost)
}

/// May a commander be hired from the pool for `cost`? The pool row's "too dear" bit (`0x00A1BB20`, flag bit 1:
/// treasury < cost compared signed, CMP/JGE at `0x00A1BB9F`), inverted. Only the interface reads it (the row's
/// `IsRecruitable`, `0x009DD3AF`, needs all its flags clear): neither the hire (`0x00A1B8F0`) nor the
/// `CanRecruitCommander` gate (`0x009D1CD0`) tests money.
pub fn commander_affordable(treasury: i32, cost: i32) -> bool {
    treasury >= cost
}

/// Charges `amount` (`0x00BAF500(amount, category)`, faction +0x3F4): a plain 32-bit subtraction, so it wraps as
/// the exe's does. The spending converter passes it unchanged (module docs). The callers are listed in the
/// module docs; none of them tests the treasury first except where its own rule says so.
pub fn pay(treasury: &mut i32, amount: i32) {
    *treasury = treasury.wrapping_sub(amount);
}

/// Credits `amount` (`0x00BB3810(amount, category)`, faction +0x3F4 plus the amount: a plain 32-bit addition, so
/// it wraps as the exe's does). The income converter passes it unchanged (module docs). A cancelled construction
/// or repair item (`0x00B1A790`, which skips a cost of 0, adding nothing anyway) or recruitment item
/// (`0x00B5C060` / `0x00B5C0A0`, item +0x20) credits its stored cost back with category 3.
pub fn credit(treasury: &mut i32, amount: i32) {
    *treasury = treasury.wrapping_add(amount);
}

/// The script's `treasury_mod(faction, amount)` (`0x0097BAD0`, CONFIRMED): the Lua number is read as an f32 and
/// rounded to an int with the x87 FISTP (`0x010556D0`, round half to even), then credited with category 3 only
/// when that int is above 0 -- a script cannot take money away.
pub fn script_credit(treasury: &mut i32, amount: f32) {
    let amount = super::commands::fistp(amount);
    if amount > 0 {
        credit(treasury, amount);
    }
}

/// The round end's cannot-pay flag (`0x00BBC7D0`, faction economics +0x464, CONFIRMED): treasury + income <
/// expenses, the sum a 32-bit wrapping addition compared signed. `income` and `expenses` are the history
/// record's plain sums (`0x00BBE970`: categories 5..11; `0x00BBE910`: 18..24).
pub fn cannot_pay(treasury: i32, income: i32, expenses: i32) -> bool {
    treasury.wrapping_add(income) < expenses
}

/// The round end's settle for a faction that can pay (`0x00BABE30`, ADD at `0x00BAC0B3`, CONFIRMED): treasury +=
/// income − expenses, 32-bit wrapping. A faction that cannot pay ([`cannot_pay`]) is not settled here (its
/// treasury goes to 0, `economy::settle_round`).
pub fn settle_round(treasury: &mut i32, income: i32, expenses: i32) {
    credit(treasury, income.wrapping_sub(expenses));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_construction_rule_is_unsigned_then_signed() {
        assert!(construction_affordable(1000, 300) && can_pay_construction(1000, 300));
        assert!(!construction_affordable(1000, -3) && !can_pay_construction(1000, -3));
        // A negative treasury passes the card test for any cost ≥ 0, the command refuses it.
        assert!(construction_affordable(-5, 300) && !can_pay_construction(-5, 300));
        assert!(construction_affordable(-5, -300) && can_pay_construction(-5, -300));
        assert!(!construction_affordable(-5, -3));
        assert!(construction_affordable(-5, i32::MIN) && can_pay_construction(-5, i32::MIN));
    }

    #[test]
    fn the_recruitment_rule_is_unsigned_only() {
        assert!(recruitment_affordable(1000, 300) && recruitment_affordable(300, 300));
        assert!(!recruitment_affordable(299, 300));
        // In debt: every cost ≥ 0 passes (the command has no signed test, unlike construction).
        assert!(recruitment_affordable(-5, 300) && recruitment_affordable(i32::MIN, i32::MAX));
    }

    #[test]
    fn paying_wraps_as_32_bit_arithmetic() {
        let mut t = -5;
        pay(&mut t, i32::MIN);
        assert_eq!(t, i32::MAX - 4);
        let mut t = 1000;
        pay(&mut t, 300);
        assert_eq!(t, 700);
        assert!(can_pay_repair(0, i32::MIN) && !can_pay_repair(0, 1));
        let mut t = 5;
        credit(&mut t, i32::MIN);
        assert_eq!(t, i32::MIN + 5);
        credit(&mut t, i32::MIN);
        assert_eq!(t, 5);
    }

    #[test]
    fn the_commander_rule_is_signed() {
        assert!(commander_affordable(400, 400) && !commander_affordable(399, 400));
        // Unlike recruitment, a faction in debt cannot afford a commander of cost ≥ 0.
        assert!(!commander_affordable(-5, 300) && commander_affordable(-5, -300));
    }

    #[test]
    fn a_script_only_adds_money_after_rounding() {
        let mut t = 100;
        script_credit(&mut t, -500.0);
        assert_eq!(t, 100, "a negative amount is ignored");
        script_credit(&mut t, 0.4);
        assert_eq!(t, 100, "rounds to 0: ignored");
        script_credit(&mut t, 2.5);
        assert_eq!(t, 102, "round half to even");
        script_credit(&mut t, 3.5);
        assert_eq!(t, 106);
        script_credit(&mut t, f32::NAN);
        assert_eq!(t, 106, "the integer indefinite is below 0");
    }

    #[test]
    fn the_round_settles_with_wrapping_sums() {
        assert!(cannot_pay(100, 50, 151) && !cannot_pay(100, 50, 150));
        // The wrapped sum is negative, so a rich faction is flagged (the exe's 32-bit compare).
        assert!(cannot_pay(i32::MAX, 1, 0));
        let mut t = 100;
        settle_round(&mut t, 50, 120);
        assert_eq!(t, 30);
        let mut t = i32::MAX;
        settle_round(&mut t, 1, 0);
        assert_eq!(t, i32::MIN);
    }
}
