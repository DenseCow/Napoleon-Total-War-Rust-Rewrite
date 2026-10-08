//! The unit-size option: how many men a unit card's men count becomes in the battle.
//! W1 §12.9 / BATTLE_FIDELITY.md §18 and §18a (the sandbox 0-A `unit_scale` sweep, CONFIRMED).
//!
//! The exe keeps the four steps as a plain float table at `0x01392770` = {0.25, 0.5, 0.75, 1.0}
//! (`0x00DAFBB0` index → scale, `0x00DAFBC0` scale → index) and multiplies the unit's card men by
//! the scale when it builds the battle unit (`0x004A6600`, the army → battle unit creator):
//!
//! ```text
//! 0x004a66c9  CALL 0x004a6540            ; clamp(scale, 0.1, 1.0)
//! 0x004a66d2  FSTP float ptr [ESP + 0x2c]
//! 0x004a67c5  MOVZX EAX,word ptr [EBP + 0xa]     ; the card's men (u16)
//! 0x004a67de  MOVD XMM0,EAX
//! 0x004a67e2  CVTDQ2PS XMM0,XMM0
//! 0x004a67e5  MULSS XMM0,dword ptr [ESP + 0x30] ; * scale   [ESP+0x30] = [ESP+0x2c] after the push
//! 0x004a67eb  CVTTSS2SI ESI,XMM0                ; (int), TRUNCATING
//! ```
//!
//! The result is what the battle unit card carries at `+0xC8`/`+0xCC` (`0x005363C0` → `0x00513320`,
//! `in_ECX[0x32] = in_ECX[0x33] = men`), and that field is the "men" the rest of the battle reads
//! (CONFIRMED: strength potentials read `card+0xC8`, §6). Naval units take a **byte** count at
//! `card+0x0E` instead and are NOT scaled (`0x004A6A06`, no MULSS on that path).
//!
//! The battle-file path is different: `0x0050CAE0` hands `num_soldiers` to the card builder
//! `0x00513440` unchanged (§39), so a `.battle` file's unit sizes are used as they are.
//!
//! Which index the battle uses (CONFIRMED code): `0x004A6540` reads the battle-settings entry with
//! key `0x0B` (`FUN_004A2B80(0x0B)`) — except in battle modes 2 and 4, where it takes the **smallest**
//! per-unit size class byte over the army's units that carry flags `+0x5D & 4`, `!(+0x5D & 8)` and
//! `+0x5D & 2` (0xFF when there are none). Whether that key is fed by the `unit_scale` preference
//! (`0x0044C9C5` loads / `0x00471B44` saves that name) is INFERRED; the model takes the scale
//! straight from the setting, which is the same value for every unit. What *sets* that per-unit
//! size class byte, and therefore the minimum modes 2 and 4 use, is still UNKNOWN.

/// The four step sizes, in index order (`0x01392770`, CONFIRMED). The names (small / medium /
/// large / ultra) are INFERRED; index 3 is the "no scaling" step.
pub const STEPS: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

/// The `gfx_unit_scale` setting index the **exe** defaults to (`0x00404230` registers it as an
/// environment variable preference — int, storage `0x0149D880`, help
/// `"Set unit scale. 0 - lowest, 3 - ultra"` — with the default 2; CONFIRMED). The player's own
/// `preferences.script.txt` ships `gfx_unit_scale 2` as well, so an out-of-the-box battle thins its
/// units to [`STEPS`][2] = 0.75 of the card's men. The model keeps 1.0 when there is no preference
/// (a deliberate deviation, see [`crate::battle::model::Battle::unit_scale`] and BATTLE_FIDELITY.md §18a).
pub const PREFERENCE_DEFAULT: i32 = 2;

/// The lowest and highest scale the exe accepts (`0x004A6540`): below `MIN` and above `MAX` the
/// value is replaced by the bound, so a stray setting can never produce 0 or > 1 men.
pub const MIN: f32 = 0.1;
/// See [`MIN`].
pub const MAX: f32 = 1.0;

/// The scale for a step index (`0x00DAFBB0`). An index outside the table is `ULTRA`
/// (`(float)(&DAT_01392770)[i]` reads past the four entries), which the clamp then keeps at 1.0.
pub fn step(index: i32) -> f32 {
    match index {
        0..=3 => STEPS[index as usize],
        _ => *STEPS.last().expect("STEPS is not empty"),
    }
}

/// The scale a battle uses, the way `0x004A6540` returns it: `if 0.1 <= s { if 1.0 <= s { 1.0 } else
/// { s } } else { 0.1 }` — i.e. a plain clamp to `[MIN, MAX]` (CONFIRMED).
pub fn clamp(scale: f32) -> f32 {
    if scale >= MAX {
        MAX
    } else if scale >= MIN {
        scale
    } else {
        MIN
    }
}

/// The battle's men for a card that carries `card_men` men: `(int)((float)card_men * scale)` with
/// the exe's **truncating** float→int conversion (`CVTTSS2SI` at `0x004A67EB`), so 160 men at
/// 0.75 is 120, not 120.5 rounded, and 1 man at 0.25 is 0 (the exe never gets there: the smallest
/// step still leaves a man for every shipped card). Negative counts are clamped to 0.
///
/// `scale` is expected to come from [`clamp`] (the caller clamps once per battle).
pub fn scaled_men(card_men: i32, scale: f32) -> u32 {
    let men = (card_men as f32 * scale) as i32;
    men.max(0) as u32
}

/// The scale a battle takes from the `gfx_unit_scale` **preference index** it was handed — the whole
/// of the decision, in one tested place.
///
/// `None` is the "no preferences file" case (no `gfx_unit_scale` value to hand), which keeps the model's 1.0 rather than guessing the exe's default; an index outside the
/// four steps reads past the table and lands on 1.0 through the clamp, exactly as in the exe.
pub fn scale_for_setting(setting: Option<i32>) -> f32 {
    setting.map_or(MAX, |i| clamp(step(i)))
}

/// [`scale_for_setting`] applied to a card's men: the count the unit is actually built with. The
/// one call the display layer needs, so the preference index, the step table, the clamp and the
/// truncation cannot drift apart between crates.
pub fn men_for_setting(card_men: i32, setting: Option<i32>) -> u32 {
    scaled_men(card_men, scale_for_setting(setting))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The step table and its clamp (`0x01392770`, `0x004A6540`).
    #[test]
    fn steps_and_clamp() {
        assert_eq!(STEPS, [0.25, 0.5, 0.75, 1.0]);
        assert_eq!((0..4).map(step).collect::<Vec<_>>(), vec![0.25, 0.5, 0.75, 1.0]);
        // An out-of-range index reads past the table; the clamp keeps the result at 1.0.
        assert_eq!(clamp(step(4)), 1.0);
        assert_eq!(clamp(step(-1)), 1.0);
        // The clamp: below 0.1 -> 0.1, above 1.0 -> 1.0, inside unchanged.
        assert_eq!(clamp(0.0), 0.1);
        assert_eq!(clamp(0.05), 0.1);
        assert_eq!(clamp(0.1), 0.1);
        assert_eq!(clamp(0.25), 0.25);
        assert_eq!(clamp(1.0), 1.0);
        assert_eq!(clamp(2.0), 1.0);
    }

    /// `CVTDQ2PS / MULSS / CVTTSS2SI` at `0x004A67DE`: the men are **truncated**, not rounded, and
    /// the small steps really do thin a unit out. 160 is the real `num_men` of a line battalion.
    #[test]
    fn men_are_truncated() {
        assert_eq!(scaled_men(160, 1.0), 160);
        assert_eq!(scaled_men(160, 0.75), 120);
        assert_eq!(scaled_men(160, 0.5), 80);
        assert_eq!(scaled_men(160, 0.25), 40);
        assert_eq!(scaled_men(155, 0.75), 116); // 116.25 -> 116
        assert_eq!(scaled_men(159, 0.5), 79); // 79.5 -> 79; rounding would give 80
        assert_eq!(scaled_men(3, 0.25), 0); // 0.75 -> 0
        // Nothing goes negative.
        assert_eq!(scaled_men(-5, 0.5), 0);
    }

    /// The four settings end to end, so the head-to-head comparison maths is pinned down: setting
    /// 0 = 0.25×, 1 = 0.5×, 2 = **0.75×**, 3 = 1.0× (no thinning). 160 men is a line battalion's
    /// real `num_men`; 160 × 0.75 = 120 exactly, and the product is always truncated.
    #[test]
    fn the_four_settings_scale_the_men() {
        for (setting, factor, men) in [(0, 0.25, 40), (1, 0.5, 80), (2, 0.75, 120), (3, 1.0, 160)] {
            let scale = clamp(step(setting));
            assert_eq!(scale, factor, "setting {setting}");
            assert_eq!(scaled_men(160, scale), men, "setting {setting}");
        }
    }

    /// The exe's own default setting, `gfx_unit_scale 2`, is **0.75** (CONFIRMED at `0x00404230`:
    /// the preference's default value is 2; the player's `preferences.script.txt` ships the same).
    #[test]
    fn default_setting_is_three_quarters() {
        assert_eq!(PREFERENCE_DEFAULT, 2);
        assert_eq!(step(PREFERENCE_DEFAULT), 0.75);
        assert_eq!(clamp(step(PREFERENCE_DEFAULT)), 0.75);
        assert_eq!(scaled_men(160, step(PREFERENCE_DEFAULT)), 120);
        assert_eq!(scaled_men(200, step(PREFERENCE_DEFAULT)), 150);
        // The exe's conversion truncates: a 7-man card at 0.75 is 5 (7 × 0.75 = 5.25), never 5.25
        // and never 6, and 158 men are 118 (118.5), not 119.
        assert_eq!(scaled_men(7, 0.75), 5);
        assert_eq!(scaled_men(158, 0.75), 118);
        // A 1-man card at the lowest step disappears (0.25 -> 0), at the default it does not
        // (0.75 -> 0 too: the exe would build a unit with no men, which never happens because
        // every shipped card has more).
        assert_eq!(scaled_men(1, 0.25), 0);
        assert_eq!(scaled_men(2, 0.75), 1); // 1.5 -> 1
        assert_eq!(scaled_men(3, 0.75), 2); // 2.25 -> 2
    }

    /// The preference index → men path, which is all the display layer has to get right: the whole
    /// decision in one call, so it can be tested here instead of in the Bevy crate (`napoleon`),
    /// which headless CI cannot build.
    #[test]
    fn the_preference_index_becomes_the_scale_and_then_the_men() {
        // No preferences file -> the model's 1.0, not a guess at the exe's default.
        assert_eq!(scale_for_setting(None), 1.0);
        assert_eq!(men_for_setting(160, None), 160);
        // The exe's default, `gfx_unit_scale 2`, really is 0.75 and really thins the men.
        assert_eq!(scale_for_setting(Some(PREFERENCE_DEFAULT)), 0.75);
        assert_eq!(men_for_setting(160, Some(PREFERENCE_DEFAULT)), 120);
        // The truncation survives the whole path: 7 * 0.75 = 5.25 -> 5, 158 -> 118.5 -> 118.
        assert_eq!(men_for_setting(7, Some(PREFERENCE_DEFAULT)), 5);
        assert_eq!(men_for_setting(158, Some(PREFERENCE_DEFAULT)), 118);
        // All four steps, and an index past the table landing on 1.0 through the clamp.
        assert_eq!([0, 1, 2, 3].map(|i| men_for_setting(160, Some(i))), [40, 80, 120, 160]);
        assert_eq!(men_for_setting(160, Some(9)), 160);
        // It agrees with `Battle::men_at_scale`, which is the other way in.
        let b = crate::battle::model::Battle::new(1, Default::default(), Default::default());
        for i in 0..4 {
            let mut b = b.clone();
            let scale = b.set_unit_scale_setting(Some(i));
            assert_eq!(scale, scale_for_setting(Some(i)), "step {i}");
            assert_eq!(b.men_at_scale(160), men_for_setting(160, Some(i)), "step {i}");
        }
        let mut none = b.clone();
        assert_eq!(none.set_unit_scale_setting(None), 1.0);
        assert_eq!(none.men_at_scale(160), 160);
    }

    /// The same factor, end to end through a real [`crate::battle::model::Battle`]: units built from a unit card at the
    /// preference default carry **three quarters** of the card's men, the truncation holds, and the
    /// thinned units are the ones the battle's own strength maths reads — so the option moves a
    /// number the battle acts on, not just a field nobody reads. 160 is a line battalion's real
    /// `num_men`, 7 the truncation case.
    ///
    /// The display layer (`crates/napoleon`, Bevy) is not wired to this yet and cannot be built
    /// headless, so the end-to-end check runs here, on the same [`crate::battle::model::Battle`]
    /// the display layer drives.
    #[test]
    fn a_battle_at_the_preference_default_really_is_thinned() {
        use crate::battle::model::{Battle, LandUnit};
        // One builder for every step, so a comparison is between two battles built the same way and
        // differs only in the setting — that is the whole claim being tested here.
        let build = |setting: Option<i32>| {
            let mut b = Battle::new(4242, Default::default(), Default::default());
            let applied = b.set_unit_scale_setting(setting);
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

        // The player's shipped `preferences.script.txt` value, applied the way the display applies it.
        let (applied, b) = build(Some(PREFERENCE_DEFAULT));
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

        // The same three cards at the other three settings: 40 / 80 / 160 men, and 7 -> 1 / 3 / 4 / 5.
        assert_eq!(build(Some(0)).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![40, 40, 1]);
        assert_eq!(build(Some(1)).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![80, 80, 3]);
        assert_eq!(build(Some(3)).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![160, 160, 7]);
        // No preferences file: the model's 1.0, the card's men untouched.
        assert_eq!(build(None).1.units.iter().map(|u| u.men).collect::<Vec<_>>(), vec![160, 160, 7]);

        // Strictly more men is strictly more strength, per side (`side_strengths` is ascending).
        let fat = build(Some(3)).1.side_strengths();
        assert_eq!(thin.iter().map(|(s, _)| *s).collect::<Vec<_>>(), fat.iter().map(|(s, _)| *s).collect::<Vec<_>>());
        for ((side, t), (_, f)) in thin.iter().zip(&fat) {
            assert!(f > t, "side {side}: unthinned {f} must beat thinned {t}");
        }
    }
}
