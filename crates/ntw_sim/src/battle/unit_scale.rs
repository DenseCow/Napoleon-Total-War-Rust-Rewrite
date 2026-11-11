//! The unit-size option: how many men a unit card's men count becomes in the battle.
//! W1 §12.9 / BATTLE_FIDELITY.md §18 and §18a.
//!
//! The exe keeps the four steps as a plain float table at `0x01392770` = {0.25, 0.5, 0.75, 1.0}
//! (`0x00DAFBB0` index → scale, `0x00DAFBC0` scale → index) and multiplies the unit's card men by
//! the scale when it builds the battle unit (`0x004A6600`, the session's army → battle unit
//! creator): it asks `0x004A6540` for the battle's scale once, then for each land unit card turns
//! the card's 16-bit men count into a float, multiplies it by the scale and converts the product
//! back with truncation (CONFIRMED, `0x004A67C5..0x004A67EB`).
//!
//! The result is what the battle unit card carries at `+0xC8`/`+0xCC` (`0x005363C0` → `0x00513320`,
//! which stores the men into dwords `0x32` and `0x33` of the card), and that field is the "men" the
//! rest of the battle reads (CONFIRMED: strength potentials read `card+0xC8`, §6). Naval units take a
//! **byte** count at `card+0x0E` instead and are NOT scaled (`0x004A6A06`, no multiplication on
//! that path).
//!
//! The battle-file path is different: `0x0050CAE0` hands `num_soldiers` to the card builder
//! `0x00513440` unchanged (§39), so a `.battle` file's unit sizes are used as they are.
//!
//! Which scale the battle uses (CONFIRMED): `0x004A6540` reads the game-settings entry with key
//! `0x0B`, a unit-size **index**, except in game modes 2 and 4, where it takes the **smallest**
//! per-slot index byte over the session's slots whose flag byte `+0x5D` has bits 4 and 2 set and
//! bit 8 clear; the index goes through `0x00DAFBB0` and the result is clamped to `[0.1, 1.0]`.
//! Key `0x0B` is written by the settings constructors only (`0x004A1930`, `0x004A1680`,
//! `0x004A14B0`): a hosted battle hands the battle setup's `unit_scale`
//! (`FrontEnd.MPHostCustomBattle`, `0x00473920`), and `LoadBattleSetup` (`0x004709F0`) fills that
//! field with the step of the player's unit-size setting, the float preference
//! `campaign_unit_multiplier` ([`crate::limits::UNIT_SIZE_SETTING`]). So the chain is setting
//! (a scale) → step index → step scale → clamp → men, which is
//! [`crate::battle::model::Battle::set_unit_scale_setting`].
//!
//! INFERRED: a single-player battle (`FrontEnd.StartBattle`, `0x004783E0` → `0x00488520`) uses the
//! same chain. Its setup tables carry no unit scale and no men count (`0x00455F00`, `0x004553B0`)
//! and it builds no game-settings object, so where its men are scaled is UNKNOWN (untraced: the
//! battle description's unit build behind `0x0050AA00`); the setting is the only unit size the
//! front end shows, so ours applies it there too. UNKNOWN: what sets the per-slot index byte modes
//! 2 and 4 read, whether an artillery unit's gun count follows the unit size (ours keeps the
//! card's guns), and what the int preference `gfx_unit_scale` (`0x00404230`, default 2) does: the
//! exe only registers it and writes it to the preferences file.

/// The original's four step sizes, in index order (`0x01392770`, CONFIRMED): the default of
/// [`crate::limits::GameLimits::unit_scales`], which a mod may change or extend. The names (small /
/// medium / large / ultra) are INFERRED; index 3 is the "no scaling" step.
pub const STEPS: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

/// The lowest and highest scale the exe accepts (`0x004A6540`): below `MIN` and above `MAX` the
/// value is replaced by the bound, so a stray setting can never produce 0 or > 1 men. Ours keeps
/// `MIN` and raises the top to a modded step above 1.0
/// ([`crate::limits::GameLimits::max_scale`]); without one the top is the original's `MAX`.
pub const MIN: f32 = 0.1;
/// See [`MIN`].
pub const MAX: f32 = 1.0;

/// The scale a battle uses, the way `0x004A6540` returns it: a plain clamp to `[MIN, top]`
/// (CONFIRMED with `top` = [`MAX`]; `top` is [`crate::limits::GameLimits::max_scale`], which is
/// `MAX` unless a mod adds a larger step).
pub fn clamp(scale: f32, top: f32) -> f32 {
    if scale >= top {
        top
    } else if scale >= MIN {
        scale
    } else {
        MIN
    }
}

/// The battle's men for a card that carries `card_men` men: `(int)((float)card_men * scale)` with
/// the exe's **truncating** float→int conversion (`0x004A67EB`), so 160 men at 0.75 is 120, not
/// 120.5 rounded, and 1 man at 0.25 is 0 (the exe never gets there: the smallest step still leaves
/// a man for every shipped card). Negative counts are clamped to 0.
///
/// `scale` is expected to come from [`clamp`] (the caller clamps once per battle).
pub fn scaled_men(card_men: i32, scale: f32) -> u32 {
    let men = (card_men as f32 * scale) as i32;
    men.max(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::GameLimits;

    /// The original's limits.
    fn l() -> GameLimits {
        GameLimits::default()
    }

    /// The step table and its clamp (`0x01392770`, `0x004A6540`).
    #[test]
    fn steps_and_clamp() {
        assert_eq!(STEPS, [0.25, 0.5, 0.75, 1.0]);
        assert_eq!((0..4).map(|i| l().unit_scale(i)).collect::<Vec<_>>(), vec![0.25, 0.5, 0.75, 1.0]);
        // An out-of-range index reads past the table; the clamp keeps the result at 1.0.
        assert_eq!(clamp(l().unit_scale(4), MAX), 1.0);
        assert_eq!(clamp(l().unit_scale(-1), MAX), 1.0);
        // The clamp: below 0.1 -> 0.1, above 1.0 -> 1.0, inside unchanged.
        assert_eq!(clamp(0.0, MAX), 0.1);
        assert_eq!(clamp(0.05, MAX), 0.1);
        assert_eq!(clamp(0.1, MAX), 0.1);
        assert_eq!(clamp(0.25, MAX), 0.25);
        assert_eq!(clamp(1.0, MAX), 1.0);
        assert_eq!(clamp(2.0, MAX), 1.0);
    }

    /// The men are **truncated** (`0x004A67DE..0x004A67EB`), not rounded, and the small steps
    /// really do thin a unit out. 160 is the real `num_men` of a line battalion.
    #[test]
    fn men_are_truncated() {
        assert_eq!(scaled_men(160, 1.0), 160);
        assert_eq!(scaled_men(160, 0.75), 120);
        assert_eq!(scaled_men(160, 0.5), 80);
        assert_eq!(scaled_men(160, 0.25), 40);
        assert_eq!(scaled_men(155, 0.75), 116); // 116.25 -> 116
        assert_eq!(scaled_men(159, 0.5), 79); // 79.5 -> 79; rounding would give 80
        assert_eq!(scaled_men(3, 0.25), 0); // 0.75 -> 0
        // A 7-man card at 0.75 is 5 (5.25), and 158 men are 118 (118.5), not 119.
        assert_eq!(scaled_men(7, 0.75), 5);
        assert_eq!(scaled_men(158, 0.75), 118);
        assert_eq!(scaled_men(2, 0.75), 1); // 1.5 -> 1
        // Nothing goes negative.
        assert_eq!(scaled_men(-5, 0.5), 0);
    }

    /// The four steps end to end: step 0 = 0.25×, 1 = 0.5×, 2 = 0.75×, 3 = 1.0× (no thinning).
    /// 160 men is a line battalion's real `num_men`.
    #[test]
    fn the_four_steps_scale_the_men() {
        for (step, factor, men) in [(0, 0.25, 40), (1, 0.5, 80), (2, 0.75, 120), (3, 1.0, 160)] {
            let scale = clamp(l().unit_scale(step), MAX);
            assert_eq!(scale, factor, "step {step}");
            assert_eq!(scaled_men(160, scale), men, "step {step}");
        }
    }

    /// The setting → men chain of a battle's start (setting scale → step `0x00DAFBC0` → step scale
    /// `0x00DAFBB0` → clamp `0x004A6540` → truncated men `0x004A6600`): the setting the game
    /// registers, 0.75, gives three quarters of the card; a setting between two steps takes the
    /// higher step; one outside the steps the first or the last. `None` (a game that registers no
    /// unit-size setting) is the last step.
    #[test]
    fn the_setting_becomes_the_step_and_then_the_men() {
        let men = |setting: Option<f32>, limits: &GameLimits| {
            let mut b = crate::battle::model::Battle::new(1, Default::default(), Default::default());
            let scale = b.set_unit_scale_setting(setting, limits);
            assert_eq!(scale, b.unit_scale);
            (scale, b.men_at_scale(160), b.men_at_scale(7))
        };
        assert_eq!(men(Some(0.75), &l()), (0.75, 120, 5));
        assert_eq!([0.25, 0.5, 1.0].map(|s| men(Some(s), &l()).1), [40, 80, 160]);
        // 0.6 is not a step: the first step not below it is 0.75. 0.05 and 3.0 are outside.
        assert_eq!(men(Some(0.6), &l()), (0.75, 120, 5));
        assert_eq!((men(Some(0.05), &l()).0, men(Some(3.0), &l()).0), (0.25, 1.0));
        assert_eq!(men(None, &l()), (1.0, 160, 7));
        // A mod's step above the original's 1.0 is reached by a setting of that size.
        let mut big = l();
        big.unit_scales.push(1.5);
        assert_eq!(men(Some(1.5), &big), (1.5, 240, 10));
        assert_eq!(men(Some(1.0), &big), (1.0, 160, 7));
    }

    /// The same factor, end to end through a real [`crate::battle::model::Battle`]: units built
    /// from a unit card at the registered setting (0.75) carry **three quarters** of the card's
    /// men, the truncation holds, and the thinned units are the ones the battle's own strength
    /// maths reads, so the option moves a number the battle acts on. 160 is a line battalion's
    /// real `num_men`, 7 the truncation case. (`napoleon`'s battle start calls the same two
    /// methods: `set_unit_scale_setting` once, `men_at_scale` per unit card.)
    #[test]
    fn a_battle_at_the_registered_setting_really_is_thinned() {
        use crate::battle::model::{Battle, LandUnit};
        // One builder for every setting, so a comparison is between two battles built the same way
        // and differs only in the setting.
        let build = |setting: f32| {
            let mut b = Battle::new(4242, Default::default(), Default::default());
            let applied = b.set_unit_scale_setting(Some(setting), &Default::default());
            // Two sides' line battalions, plus a 7-man card for the truncation.
            for (id, side, card_men) in [(1u32, 0u8, 160i32), (2, 1, 160), (3, 0, 7)] {
                let men = b.men_at_scale(card_men);
                let mut u = LandUnit::new(id, side, men, (0.0, 0.0));
                // Real-ish card values so the strength maths is not a flat zero.
                u.melee_attack = 10;
                u.melee_defence = 8;
                u.unit_category = "infantry".into();
                b.add_unit(u);
            }
            (applied, b)
        };

        let (applied, b) = build(0.75);
        assert_eq!(applied, 0.75);
        assert_eq!(b.unit_scale, 0.75);
        // Three quarters of the card, truncated: 160 -> 120 and 7 -> 5.
        assert_eq!(b.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![120, 120, 5]);
        assert_eq!(b.units[2].max_men, 5);
        // And the option is visible in what the battle computes: the alliance strength `0x00539E80`
        // reads is built from the men, so thinning the men thins the strength.
        let thin = b.side_strengths();
        assert_eq!(thin.len(), 2);
        assert!(thin.iter().all(|(_, s)| *s > 0.0));

        // The same three cards at the other three settings: 40 / 80 / 160 men, and 7 -> 1 / 3 / 7.
        assert_eq!(build(0.25).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![40, 40, 1]);
        assert_eq!(build(0.5).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![80, 80, 3]);
        assert_eq!(build(1.0).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![160, 160, 7]);

        // Strictly more men is strictly more strength, per side (`side_strengths` is ascending).
        let fat = build(1.0).1.side_strengths();
        assert_eq!(thin.iter().map(|(s, _)| *s).collect::<Vec<_>>(), fat.iter().map(|(s, _)| *s).collect::<Vec<_>>());
        for ((side, t), (_, f)) in thin.iter().zip(&fat) {
            assert!(f > t, "side {side}: unthinned {f} must beat thinned {t}");
        }
    }
}
