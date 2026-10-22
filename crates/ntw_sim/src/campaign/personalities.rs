//! The campaign AI's personalities (`campaign_ai_personalities` and their
//! `campaign_ai_personality_junctions` tunables, through the merged database): one table the model's AI
//! rules ([`super::capture_ai`]) and the campaign AI (`ntw_ai`) both read.

use std::collections::BTreeMap;

/// The personalities and their tunables ([`super::CampaignRules::ai_personalities`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiPersonalities {
    /// Personality key → its own tunables (a personality's rows hold only the keys it sets). The exe's
    /// loader `0x00DD5870` writes the default's rows into every record first, then each record's own rows
    /// (CONFIRMED), which [`Self::tunable`]'s own-else-default lookup gives.
    pub by_key: BTreeMap<String, BTreeMap<String, f32>>,
    /// The default personality: the first `campaign_ai_personalities` record whose #1 is set (record +0xC),
    /// CONFIRMED: `0x00DD5870` keeps the first flagged record (`0x00DD58DC`) and applies its junction rows to
    /// every personality.
    pub default: String,
}

impl AiPersonalities {
    /// Whether the personality exists.
    pub fn contains(&self, personality: &str) -> bool {
        self.by_key.contains_key(personality)
    }

    /// A tunable: the personality's own value, else the default personality's.
    pub fn tunable(&self, personality: &str, key: &str) -> Option<f32> {
        self.by_key
            .get(personality)
            .and_then(|p| p.get(key))
            .or_else(|| self.by_key.get(&self.default).and_then(|p| p.get(key)))
            .copied()
    }

    /// The personality a faction runs: its stored key (`FACTION` record, CONFIRMED source) when it is a
    /// known personality, else the naming rule `<prefix>_<faction>` when that exists, else the default.
    /// `<prefix>` is the campaign key's part after an optional `mp_` and before the first `_` (e.g. `eur`
    /// from `eur_napoleon`). PROVISIONAL: the original's fallback for a missing or unknown stored key is
    /// UNKNOWN (it logs "not a valid key for this table"); the shipped data fits the naming rule.
    pub fn resolve(&self, campaign_key: &str, faction_key: &str, stored: Option<&str>) -> String {
        if let Some(k) = stored.filter(|k| self.contains(k)) {
            return k.to_owned();
        }
        let c = campaign_key.strip_prefix("mp_").unwrap_or(campaign_key);
        let prefix = c.split('_').next().unwrap_or(c);
        let named = format!("{prefix}_{faction_key}");
        if self.contains(&named) { named } else { self.default.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> AiPersonalities {
        let mut p = AiPersonalities { default: "default".into(), ..Default::default() };
        p.by_key.entry("default".into()).or_default().insert("A".into(), 0.25);
        p.by_key.entry("default".into()).or_default().insert("B".into(), 1.0);
        p.by_key.entry("eur_france".into()).or_default().insert("B".into(), 0.0);
        p
    }

    #[test]
    fn a_tunable_falls_back_to_the_default_personality() {
        let p = data();
        assert_eq!(p.tunable("eur_france", "B"), Some(0.0), "own value wins");
        assert_eq!(p.tunable("eur_france", "A"), Some(0.25), "default's value");
        assert_eq!(p.tunable("nobody", "A"), Some(0.25));
        assert_eq!(p.tunable("eur_france", "C"), None);
    }

    #[test]
    fn a_faction_resolves_its_stored_key_then_the_naming_rule_then_the_default() {
        let p = data();
        assert_eq!(p.resolve("eur_napoleon", "britain", Some("eur_france")), "eur_france");
        assert_eq!(p.resolve("mp_eur_napoleon", "france", Some("unknown")), "eur_france");
        assert_eq!(p.resolve("eur_napoleon", "france", None), "eur_france");
        assert_eq!(p.resolve("ita_napoleon", "france", None), "default");
    }
}
