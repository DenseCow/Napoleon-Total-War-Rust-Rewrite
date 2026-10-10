//! The importer's campaign rule switches: the original's campaign keys mapped to the
//! [`CampaignFeatures`] the exe hard-codes for them (MODDING_AUDIT.md §1.1). The model reads the
//! table, never the key, so a campaign of another source sets its own.

use ntw_sim::campaign::features::{CampaignFeatures, UnitKeyEffects};

/// The Peninsular campaign's key: the exe's switches below test it (each CONFIRMED at the address the
/// feature's field names).
const PENINSULAR: &str = "spa_napoleon";
/// The multiplayer Europe campaign's key (`0x00B61F30`, CONFIRMED).
const MP_EUROPE: &str = "mp_eur_napoleon";

/// The feature set the exe gives the original's campaign `key`; any other key gets none (the exe's
/// behaviour for every campaign it has no switch for).
pub fn original(key: &str) -> CampaignFeatures {
    let mut f = CampaignFeatures::default();
    if key == PENINSULAR {
        f.loot_value = (2, 10_000);
        f.looting_alignment = Some(("align_pro_french".into(), "align_anti_french".into()));
        f.unit_key_effects = vec![
            UnitKeyEffects::new("_Guerrilla", "guerrilla_cost_mod", "guerrilla_upkeep_mod"),
            UnitKeyEffects::new("_Auxiliary", "auxiliary_cost_mod", "auxiliary_upkeep_mod"),
        ];
        f.alignment_public_order = true;
        f.faction_zeal = true;
        f.home_trade_faction = Some("spa_france".into());
        f.node_supply_mod = true;
        f.fixed_commodity_prices = true;
    }
    if key == MP_EUROPE {
        f.ai_recruitment_points = vec![("france".into(), 1)];
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the two switched campaigns get features; the others behave as the exe's default.
    #[test]
    fn the_original_keys_get_the_exe_switches() {
        for key in ["eur_napoleon", "egy_napoleon", "ita_napoleon", "mp_ita_napoleon", "made_up_campaign"] {
            assert_eq!(original(key), CampaignFeatures::default(), "{key}");
        }
        let spa = original("spa_napoleon");
        assert_eq!(spa.loot_value, (2, 10_000));
        assert_eq!(spa.home_trade_faction.as_deref(), Some("spa_france"));
        assert_eq!(spa.effects_of_unit("Inf_Spanish_Guerrilla").map(|e| e.cost.as_str()), Some("guerrilla_cost_mod"));
        assert_eq!(spa.effects_of_unit("Inf_British_Auxiliary").map(|e| e.cost.as_str()), Some("auxiliary_cost_mod"));
        assert_eq!(spa.effects_of_unit("Inf_Spanish_guerrilla"), None, "case-sensitive");
        assert_eq!(spa.effects_of_unit("Inf_British_Auxiliary").map(|e| e.upkeep.as_str()), Some("auxiliary_upkeep_mod"));
        assert!(spa.alignment_public_order && spa.faction_zeal && spa.node_supply_mod && spa.fixed_commodity_prices);
        let mp = original("mp_eur_napoleon");
        assert_eq!(mp.ai_recruitment_bonus("france"), 1);
        assert_eq!(mp.ai_recruitment_bonus("britain"), 0);
        assert_eq!(CampaignFeatures { ai_recruitment_points: Vec::new(), ..mp }, CampaignFeatures::default());
    }

    /// A campaign's own data can name any feature set: it round-trips through the open text format.
    #[test]
    fn a_feature_set_reads_from_text() {
        let text = "(loot_value: (3, 9000), home_trade_faction: Some(\"shogunate\"), ai_recruitment_points: [(\"oda\", 2)])";
        let f: CampaignFeatures = ron::from_str(text).expect("parse");
        assert_eq!(f.loot_value, (3, 9000));
        assert_eq!(f.home_trade_faction.as_deref(), Some("shogunate"));
        assert_eq!(f.ai_recruitment_bonus("oda"), 2);
        assert!(!f.faction_zeal, "fields left out keep the default");
        assert_eq!(ron::from_str::<CampaignFeatures>(&ron::to_string(&original("spa_napoleon")).unwrap()).unwrap(), original("spa_napoleon"));
    }
}
