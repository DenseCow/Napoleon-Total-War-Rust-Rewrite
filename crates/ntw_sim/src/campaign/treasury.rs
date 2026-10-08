//! The original's construction and repair money rules (CAMPAIGN_FIDELITY.md §Construction cost): their
//! affordability, their charge and the refund of a cancelled item. Only construction and repair go through
//! here; recruitment, the unit pool, treaties, upkeep and script treasury changes keep their own arithmetic
//! (BACKLOG §0-B: not traced).

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

/// Charges `amount` (`0x00BAF500`, faction +0x3F4): a plain 32-bit subtraction, so it wraps as the exe's does.
/// INFERRED: the amount goes through the spending category's converter (`0x00BA8CD0`, vtable +4) unchanged for
/// the category 0 construction and repair use.
pub fn pay(treasury: &mut i32, amount: i32) {
    *treasury = treasury.wrapping_sub(amount);
}

/// Credits a cancelled construction or repair item's stored cost back (`0x00B1A790` → `0x00BB3810(cost, 3)`,
/// faction +0x3F4 plus the amount: a plain 32-bit addition, so it wraps as the exe's does). The exe skips a cost
/// of 0, which adds nothing anyway. INFERRED: the income category 3 converter (`0x00BA8CE0`, vtable +4) passes the
/// amount unchanged.
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
