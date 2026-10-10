//! The per-campaign rule switches (MODDING_AUDIT.md §1.1).
//!
//! The original hard-codes a few rules by campaign or faction key (`spa_napoleon`, `mp_eur_napoleon`,
//! `spa_france`; each site below names its exe address, all CONFIRMED). The model reads them from
//! this table instead of the key, so a custom or mod campaign can switch any of them on or off in its
//! own data. The original's importer fills it from the campaign key exactly as the exe switches
//! (`ntw_campaign::features::original`); [`CampaignFeatures::default`] is a campaign the exe has no
//! switch for (e.g. `eur_napoleon`).

/// One campaign's rule switches. See the module docs; every field's default is the exe's behaviour
/// outside the switched campaigns.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct CampaignFeatures {
    /// The capture preview's building loot value: (multiplier of the lost health's value, cap)
    /// (`0x00B14930` through [`super::capture::damage_roll`], CONFIRMED: `spa_napoleon` (2, 10000),
    /// every other campaign (4, 15000)).
    pub loot_value: (u32, u32),
    /// Looting turns the population against the looter (`0x00AAA5B0`, CONFIRMED in `spa_napoleon`):
    /// the two alignment religion keys (`align_pro_french`, `align_anti_french`). The looter's own
    /// religion loses 0.5 of the share, the other gains it. `None`: no shift.
    pub looting_alignment: Option<(String, String)>,
    /// Extra recruitment cost and upkeep effects by the end of the unit key, tested in order, the first
    /// match only, case-sensitive (CONFIRMED in `spa_napoleon`: `_Guerrilla` → `guerrilla_cost_mod` /
    /// `guerrilla_upkeep_mod`, else `_Auxiliary` → `auxiliary_cost_mod` / `auxiliary_upkeep_mod`; cost
    /// `0x00B0D220`, land upkeep `0x008F9B10`, effects 0x90 / 0x94).
    pub unit_key_effects: Vec<UnitKeyEffects>,
    /// Public order: the religion factor goes to happiness slot 12 instead of 2, and hostile armies in
    /// the region add no war-results unrest (`spa_napoleon`, CONFIRMED; [`super::economy`]).
    pub alignment_public_order: bool,
    /// A missionary's rank also adds his faction's `zeal_europe` ([`super::religion::missionary_rank`]).
    /// The exe (`0x00A198D0`, CONFIRMED) tests the theatre under the missionary (`spain_main`); the
    /// model has no theatres (one per shipped map) and `spa_napoleon`'s map is the `spain_main` one.
    pub faction_zeal: bool,
    /// The faction that also earns its home trade value (`0x00BB3490`, CONFIRMED: `spa_france` in
    /// `spa_napoleon`; [`super::CampaignModel::trade_home_value`]).
    pub home_trade_faction: Option<String>,
    /// A trade node's fleet supply is scaled by the faction's `trade_node_supply_mod` (`0x00BC9930`,
    /// CONFIRMED in `spa_napoleon`; [`super::CampaignModel::node_supply`]).
    pub node_supply_mod: bool,
    /// Commodity prices are never updated once set (CONFIRMED in `spa_napoleon`;
    /// [`super::CampaignModel::update_commodity_prices`]).
    pub fixed_commodity_prices: bool,
    /// Extra land recruitment points in every region of an AI-run faction, by faction key
    /// (`0x00B61F30`, CONFIRMED: `france` +1 in `mp_eur_napoleon`;
    /// [`super::CampaignModel::recruitment_points`]).
    pub ai_recruitment_points: Vec<(String, i32)>,
    /// The calendar's date step per turn in quarter-months (a campaign's own calendar, e.g. 12 for four
    /// turns a year). `None`: the exe's step for the calendar's `turns_per_year`
    /// ([`crate::calendar::original_date_step`]).
    pub date_step: Option<u32>,
}

impl Default for CampaignFeatures {
    fn default() -> Self {
        CampaignFeatures {
            loot_value: (4, 15_000),
            looting_alignment: None,
            unit_key_effects: Vec::new(),
            alignment_public_order: false,
            faction_zeal: false,
            home_trade_faction: None,
            node_supply_mod: false,
            fixed_commodity_prices: false,
            ai_recruitment_points: Vec::new(),
            date_step: None,
        }
    }
}

impl CampaignFeatures {
    /// The effects of `unit_key` from [`Self::unit_key_effects`] (the first matching suffix).
    pub fn effects_of_unit(&self, unit_key: &str) -> Option<&UnitKeyEffects> {
        self.unit_key_effects.iter().find(|e| unit_key.ends_with(e.suffix.as_str()))
    }

    /// The extra land recruitment points of an AI-run faction `faction_key` ([`Self::ai_recruitment_points`]).
    pub fn ai_recruitment_bonus(&self, faction_key: &str) -> i32 {
        self.ai_recruitment_points.iter().filter(|(k, _)| k == faction_key).map(|(_, p)| *p).sum()
    }
}

/// The extra effects of the units whose key ends in `suffix` ([`CampaignFeatures::unit_key_effects`]).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnitKeyEffects {
    /// The end of the unit key, e.g. `_Guerrilla` (case-sensitive).
    pub suffix: String,
    /// The integer effect added to the recruitment cost mod, e.g. `guerrilla_cost_mod`.
    pub cost: String,
    /// The integer effect added to the land upkeep mod, e.g. `guerrilla_upkeep_mod`.
    pub upkeep: String,
}

impl UnitKeyEffects {
    /// A row from its three keys.
    pub fn new(suffix: &str, cost: &str, upkeep: &str) -> Self {
        UnitKeyEffects { suffix: suffix.to_owned(), cost: cost.to_owned(), upkeep: upkeep.to_owned() }
    }
}
