//! The importer's campaign rule switches: the original's campaign keys mapped to the
//! [`CampaignFeatures`] the exe hard-codes for them (MODDING_AUDIT.md §1.1). The model reads the
//! table, never the key, so a campaign of another source sets its own.

use ntw_sim::campaign::features::{AiCaptureLiberation, CampaignFeatures, LootResentment, PeaceAccessGrant, UnitKeyEffects};

/// The Peninsular campaign's key: the exe's switches below test it (each CONFIRMED at the address the
/// feature's field names).
const PENINSULAR: &str = "spa_napoleon";
/// The multiplayer Europe campaign's key (`0x00B61F30`, CONFIRMED).
const MP_EUROPE: &str = "mp_eur_napoleon";
/// The campaigns whose default save name starts with the campaign's on-screen name
/// (`BuildDefaultSaveName` `0x009D5400` compares the key with these four, CONFIRMED).
const SAVE_NAMED_BY_CAMPAIGN: [&str; 4] = ["egy_napoleon", "ita_napoleon", "tut_napoleon", "eur_napoleon"];
/// The Empire campaign key whose diplomacy screen hides `britain` (`0x009F2EA0` at `0x009F2F77`,
/// CONFIRMED); no Napoleon campaign has it, so vanilla never hides a faction.
const EPISODIC_3: &str = "episodic_3";
/// The campaign-key prefix whose building effects tables show no technologies (`0x009B2BB0`, string
/// `0x0136C4F8` tested at `0x009B2FD9`, CONFIRMED); no Napoleon campaign key has it.
const EPISODIC_PREFIX: &str = "episodic";

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
        f.loot_resentment = Some(LootResentment {
            homelands: [("spa_portugal", "spa_portugal"), ("portugese_rebels", "spa_portugal"), ("spa_spain", "spa_spain")]
                .map(|(r, f)| (r.to_owned(), f.to_owned()))
                .to_vec(),
            exempt_looter: Some("spa_france".into()),
            factor: "abandoned_ally_in_war".into(),
            value: -50,
        });
        // `MakePeaceWithFaction` 0x00B262C0: an Iberian peace with Britain gives Britain indefinite access.
        f.peace_access_grants = vec![PeaceAccessGrant {
            granters: vec!["spa_spain".into(), "spa_portugal".into()],
            grantee: "spa_britain".into(),
            turns: -1,
        }];
        // The AI peace attitude `0x00D63260`: no peace with a human the AI hates (total below −199).
        f.ai_peace_refusal_attitude = Some(-200);
        // The AI capture choice `0x00D8C8B0`: Portugal liberates Spain; Britain may, by the French border test.
        f.ai_capture_liberation = Some(AiCaptureLiberation {
            liberator: "spa_portugal".into(),
            contested_liberator: "spa_britain".into(),
            target: "spa_spain".into(),
            rival: "spa_france".into(),
        });
    }
    if key == MP_EUROPE {
        f.ai_recruitment_points = vec![("france".into(), 1)];
    }
    f.save_name_uses_campaign_name = SAVE_NAMED_BY_CAMPAIGN.contains(&key);
    if key == EPISODIC_3 {
        f.diplomacy_hidden_factions = vec!["britain".into()];
    }
    f.effects_technologies_hidden = key.starts_with(EPISODIC_PREFIX);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the two switched campaigns get rule features, and the four single-player Napoleon
    /// campaigns the save-name switch; the others behave as the exe's default.
    #[test]
    fn the_original_keys_get_the_exe_switches() {
        for key in ["eur_napoleon", "egy_napoleon", "ita_napoleon", "tut_napoleon"] {
            let f = original(key);
            assert!(f.save_name_uses_campaign_name, "{key}");
            assert_eq!(CampaignFeatures { save_name_uses_campaign_name: false, ..f }, CampaignFeatures::default(), "{key}");
        }
        for key in ["mp_ita_napoleon", "made_up_campaign"] {
            assert_eq!(original(key), CampaignFeatures::default(), "{key}");
        }
        assert!(!original("spa_napoleon").save_name_uses_campaign_name, "the Peninsular campaign takes the faction's name");
        let spa = original("spa_napoleon");
        assert_eq!(spa.loot_value, (2, 10_000));
        assert_eq!(spa.home_trade_faction.as_deref(), Some("spa_france"));
        assert_eq!(spa.effects_of_unit("Inf_Spanish_Guerrilla").map(|e| e.cost.as_str()), Some("guerrilla_cost_mod"));
        assert_eq!(spa.effects_of_unit("Inf_British_Auxiliary").map(|e| e.cost.as_str()), Some("auxiliary_cost_mod"));
        assert_eq!(spa.effects_of_unit("Inf_Spanish_guerrilla"), None, "case-sensitive");
        assert_eq!(spa.effects_of_unit("Inf_British_Auxiliary").map(|e| e.upkeep.as_str()), Some("auxiliary_upkeep_mod"));
        assert!(spa.alignment_public_order && spa.faction_zeal && spa.node_supply_mod && spa.fixed_commodity_prices);
        let lr = spa.loot_resentment.as_ref().expect("loot resentment");
        assert_eq!(lr.homelands.iter().map(|(_, f)| f.as_str()).collect::<Vec<_>>(), ["spa_portugal", "spa_portugal", "spa_spain"]);
        assert_eq!((lr.exempt_looter.as_deref(), lr.factor.as_str(), lr.value), (Some("spa_france"), "abandoned_ally_in_war", -50));
        let [grant] = spa.peace_access_grants.as_slice() else { panic!("one peace access grant") };
        assert_eq!((grant.granters.as_slice(), grant.grantee.as_str(), grant.turns), (&["spa_spain".to_owned(), "spa_portugal".to_owned()][..], "spa_britain", -1));
        assert_eq!(spa.ai_peace_refusal_attitude, Some(-200));
        let lib = spa.ai_capture_liberation.as_ref().expect("AI capture liberation");
        assert_eq!((lib.liberator.as_str(), lib.contested_liberator.as_str(), lib.target.as_str(), lib.rival.as_str()), ("spa_portugal", "spa_britain", "spa_spain", "spa_france"));
        let mp = original("mp_eur_napoleon");
        assert_eq!(mp.ai_recruitment_bonus("france"), 1);
        assert_eq!(mp.ai_recruitment_bonus("britain"), 0);
        assert_eq!(CampaignFeatures { ai_recruitment_points: Vec::new(), ..mp }, CampaignFeatures::default());
        assert_eq!(original("episodic_3").diplomacy_hidden_factions, ["britain"]);
        for key in ["episodic_1", "episodic_3"] {
            assert!(original(key).effects_technologies_hidden, "{key}");
        }
        assert!(!original("Episodic_1").effects_technologies_hidden, "case-sensitive prefix");
        assert_eq!(CampaignFeatures { effects_technologies_hidden: false, ..original("episodic_1") }, CampaignFeatures::default());
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

    /// A loot resentment row naming an unknown attitude factor is dropped once, when the rules load; the
    /// Peninsular row is kept.
    #[test]
    fn an_unknown_loot_resentment_factor_is_dropped_at_load() {
        let spa = original("spa_napoleon");
        assert_eq!(spa.clone().validated(), spa);
        let mut bad = spa.clone();
        bad.loot_resentment.as_mut().unwrap().factor = "made_up_factor".into();
        assert_eq!(bad.validated(), CampaignFeatures { loot_resentment: None, ..spa });
    }
}
