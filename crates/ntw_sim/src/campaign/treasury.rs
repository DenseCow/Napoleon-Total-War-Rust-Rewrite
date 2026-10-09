//! The original's treasury money rules (CAMPAIGN_FIDELITY.md §Construction cost, §Recruitment): the
//! affordability of construction, repair and recruitment, their charge and the refund of a cancelled item.
//! The unit pool, treaties, upkeep and script treasury changes keep their own arithmetic (BACKLOG §0-B: not
//! traced).
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

/// Charges `amount` (`0x00BAF500`, faction +0x3F4): a plain 32-bit subtraction, so it wraps as the exe's does.
/// The spending converter passes it unchanged (module docs): category 0 for construction and repair, 2 for
/// recruitment (`0x00B58DD0`).
pub fn pay(treasury: &mut i32, amount: i32) {
    *treasury = treasury.wrapping_sub(amount);
}

/// Credits a cancelled item's stored cost back (`0x00BB3810(cost, 3)`, faction +0x3F4 plus the amount: a plain
/// 32-bit addition, so it wraps as the exe's does): a construction or repair item (`0x00B1A790`, which skips a
/// cost of 0, adding nothing anyway) or a recruitment item (`0x00B5C060` / `0x00B5C0A0`, item +0x20). The income
/// category 3 converter passes it unchanged (module docs).
pub fn refund(treasury: &mut i32, amount: i32) {
    *treasury = treasury.wrapping_add(amount);
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
        refund(&mut t, i32::MIN);
        assert_eq!(t, i32::MIN + 5);
        refund(&mut t, i32::MIN);
        assert_eq!(t, 5);
    }
}
