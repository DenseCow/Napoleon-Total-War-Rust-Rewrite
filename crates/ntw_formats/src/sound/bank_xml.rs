//! The sound-bank XML sources (`sounds\banks\sound_bank_*.xml`) and the condition names
//! they give to the numbers in the packed bank database.
//!
//! The packed database ([`super::banks`]) stores condition values as bare numbers (enum
//! values inside the exe). The XML sources it was built from ship in the same pack and
//! spell the same conditions out by name, entry by entry and in the same order:
//! ```xml
//! <sound_bank_event sound_event_name ="CANNON_Fire_Close">
//!   <gun_type>cannon</gun_type> <gun_type>howitzer</gun_type> <gun_type>none</gun_type>
//!   <shot_type>round_shot</shot_type> ...
//!   <audio_distance>close</audio_distance>
//! </sound_bank_event>
//! ```
//! packs to `event 3428, [[2, 7, 19], [0, 7, 8, ...], [0], []]`. Pairing the two gives
//! each (bank type, condition list) a tag name and a name -> number table, so callers can
//! ask for "gun_type musket_flintlock" instead of a magic number (INFERRED method, checked
//! on every shipped bank: AUDIO_FORMAT.md §4.4). A bank with no matching XML still works by
//! number.

use std::collections::{BTreeMap, HashMap};

use super::banks::SoundBankDatabase;
use super::names::EventNames;

/// The folder of the bank XML sources.
pub const BANK_XML_DIR: &str = r"sounds\banks";

/// One `<sound_bank_event>` of a bank XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlBankEvent {
    pub event_name: String,
    /// `(tag, text)` children in file order.
    pub conditions: Vec<(String, String)>,
}

/// Parses a bank XML file into its `<sound_bank_event>` list. Comments are skipped.
pub fn parse_bank_xml(text: &str) -> Vec<XmlBankEvent> {
    let mut out = Vec::new();
    let mut cur: Option<XmlBankEvent> = None;
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        if let Some(body) = rest.strip_prefix("<!--") {
            // `<!---- X ---->` style comments end at the first "-->".
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if let Some(t) = tag.strip_prefix("sound_bank_event") {
            let name = t.split_once('"').and_then(|(_, r)| r.split_once('"')).map(|(n, _)| n.trim().to_owned()).unwrap_or_default();
            cur = Some(XmlBankEvent { event_name: name, conditions: Vec::new() });
        } else if tag.starts_with("/sound_bank_event") {
            if let Some(e) = cur.take() {
                out.push(e);
            }
        } else if let Some(e) = cur.as_mut()
            && !tag.starts_with('/')
            && !tag.ends_with('/')
        {
            let name = tag.trim().to_owned();
            let close = format!("</{name}>");
            if let Some(end) = rest.find(&close) {
                e.conditions.push((name, rest[..end].trim().to_owned()));
                rest = &rest[end + close.len()..];
            }
        }
    }
    out
}

/// Pairs XML entries with the packed entries of `bank`, in order. An XML entry pairs with
/// the next packed entry when its event name names that entry's event, or when it
/// carries the same number of distinct condition values; other XML entries
/// are skipped (the builder dropped them, e.g. commented-out or unresolvable events).
/// Returns the paired XML entries (one per packed entry) and how many paired by name,
/// or `None` if some packed entry found no partner.
pub fn align<'a>(src: &'a [XmlBankEvent], bank: &super::banks::SoundBank, names: &EventNames) -> Option<(Vec<&'a XmlBankEvent>, usize)> {
    let mut paired = Vec::with_capacity(bank.entries.len());
    let mut agree = 0;
    for x in src {
        let Some(b) = bank.entries.get(paired.len()) else { break };
        let cands = names.find(&x.event_name);
        let by_name = cands.contains(&(b.event as usize));
        let values: usize = b.conditions.iter().map(Vec::len).sum();
        let mut distinct: Vec<&(String, String)> = x.conditions.iter().collect();
        distinct.sort();
        distinct.dedup();
        if by_name || values == distinct.len() {
            paired.push(x);
            agree += usize::from(by_name);
        }
    }
    if paired.len() == bank.entries.len() {
        return Some((paired, agree));
    }
    // Fallback: same number of entries, paired by position.
    (src.len() == bank.entries.len()).then(|| {
        let agree = src.iter().zip(&bank.entries).filter(|(x, b)| names.find(&x.event_name).contains(&(b.event as usize))).count();
        (src.iter().collect(), agree)
    })
}

/// The names of one condition list of one bank type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConditionNames {
    /// The XML tag, e.g. `"shot_type"`.
    pub tag: String,
    /// Lower-cased value name -> number.
    pub values: BTreeMap<String, u32>,
}

/// What the XML sources say about one bank type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BankNames {
    /// The XML file (e.g. `sound_bank_projectile_fire`), lower-case, without folder or extension.
    pub source: String,
    /// One entry per condition list (`None` if no tag lines up with that list).
    pub conditions: Vec<Option<ConditionNames>>,
}

/// Condition names for every bank type that has a matching XML source.
#[derive(Debug, Clone, Default)]
pub struct BankVocabulary {
    pub banks: HashMap<u32, BankNames>,
}

impl BankVocabulary {
    /// Pairs the XML sources `xmls` (`(path, text)`) with the packed banks.
    ///
    /// A source belongs to a bank type when it has the same number of entries and every
    /// entry's event name (when [`EventNames`] knows it) names the packed entry's event.
    pub fn build(db: &SoundBankDatabase, names: &EventNames, xmls: &[(String, String)]) -> Self {
        let parsed: Vec<(String, Vec<XmlBankEvent>)> = xmls
            .iter()
            .map(|(p, t)| {
                let stem = p.rsplit(['\\', '/']).next().unwrap_or(p);
                let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_ascii_lowercase();
                (stem, parse_bank_xml(t))
            })
            .collect();
        let mut banks = HashMap::new();
        for bank in &db.banks {
            if bank.entries.is_empty() {
                continue;
            }
            // Best source: the one that pairs every packed entry with the fewest XML entries
            // left over, and the most entries whose event name agrees.
            let mut best: Option<((usize, usize), &String, Vec<&XmlBankEvent>)> = None;
            for (stem, src) in &parsed {
                let Some((paired, agree)) = align(src, bank, names) else { continue };
                let score = (agree, usize::MAX - (src.len() - paired.len()));
                if best.as_ref().is_none_or(|(s, _, _)| score > *s) {
                    best = Some((score, stem, paired));
                }
            }
            let Some(((agree, _), stem, xml)) = best else { continue };
            // Require most named entries to agree (unknown events are -1 in the packed data).
            let known = bank.entries.iter().filter(|b| names.name(b.event as usize).is_some()).count();
            if agree * 2 < known {
                continue;
            }
            let lists = bank.entries[0].conditions.len();
            let mut conditions = Vec::with_capacity(lists);
            let mut used: Vec<String> = Vec::new();
            for k in 0..lists {
                // Tag whose value count matches list k in every entry (and is used at least once).
                let mut tags: Vec<&str> = xml.iter().flat_map(|x| x.conditions.iter().map(|(t, _)| t.as_str())).collect();
                tags.sort_unstable();
                tags.dedup();
                let fits = |tag: &str| {
                    let mut used_once = false;
                    for (x, b) in xml.iter().zip(&bank.entries) {
                        let mut vals: Vec<&str> = x.conditions.iter().filter(|(t, _)| t == tag).map(|(_, v)| v.as_str()).collect();
                        vals.sort_unstable();
                        vals.dedup();
                        let n = vals.len();
                        if n != b.conditions[k].len() {
                            return false;
                        }
                        used_once |= n > 0;
                    }
                    used_once
                };
                let tag = tags.into_iter().find(|t| !used.iter().any(|u| u == t) && fits(t));
                conditions.push(tag.map(|tag| {
                    used.push(tag.to_owned());
                    let mut values = BTreeMap::new();
                    for (x, b) in xml.iter().zip(&bank.entries) {
                        let mut texts: Vec<String> = Vec::new();
                        for (t, v) in &x.conditions {
                            let v = v.to_ascii_lowercase();
                            if t == tag && !texts.contains(&v) {
                                texts.push(v);
                            }
                        }
                        for (name, &num) in texts.into_iter().zip(&b.conditions[k]) {
                            values.entry(name).or_insert(num);
                        }
                    }
                    ConditionNames { tag: tag.to_owned(), values }
                }));
            }
            banks.insert(bank.bank_type, BankNames { source: stem.clone(), conditions });
        }
        Self { banks }
    }

    /// The bank type built from the XML source with this stem (e.g. `"sound_bank_projectile_fire"`).
    pub fn bank_type_of(&self, source: &str) -> Option<u32> {
        self.banks.iter().find(|(_, b)| b.source.eq_ignore_ascii_case(source)).map(|(t, _)| *t)
    }

    /// Index of the condition list with this tag in a bank type.
    pub fn condition_index(&self, bank_type: u32, tag: &str) -> Option<usize> {
        self.banks.get(&bank_type)?.conditions.iter().position(|c| c.as_ref().is_some_and(|c| c.tag.eq_ignore_ascii_case(tag)))
    }

    /// The number of a named condition value (case-insensitive). Allocates nothing.
    pub fn value(&self, bank_type: u32, tag: &str, name: &str) -> Option<u32> {
        let k = self.condition_index(bank_type, tag)?;
        self.value_at(bank_type, k, name)
    }

    /// [`value`](Self::value) in condition list `k`. The stored names are lower-cased
    /// ([`ConditionNames::values`]), so a lower-case `name` is one map lookup and any other a
    /// case-insensitive scan, never a new string.
    fn value_at(&self, bank_type: u32, k: usize, name: &str) -> Option<u32> {
        let values = &self.banks.get(&bank_type)?.conditions.get(k)?.as_ref()?.values;
        if name.bytes().any(|b| b.is_ascii_uppercase()) {
            values.iter().find(|(v, _)| v.eq_ignore_ascii_case(name)).map(|(_, &n)| n)
        } else {
            values.get(name).copied()
        }
    }

    /// Builds a query for [`super::SoundBank::matching`] from `(tag, value name)` pairs.
    /// Returns `None` if a tag or value is unknown.
    pub fn query(&self, bank_type: u32, conditions: &[(&str, &str)]) -> Option<Vec<Option<u32>>> {
        let mut q = Vec::new();
        self.build_query(bank_type, conditions, true, &mut q).then_some(q)
    }

    /// A query like [`query`](Self::query), written into `out` (its buffer reused, so a caller
    /// that keeps `out` allocates nothing after its first call), with each pair whose tag or value
    /// is unknown left out instead of failing the query. False, `out` untouched, when the bank
    /// type has no names.
    pub fn query_known_into(&self, bank_type: u32, conditions: &[(&str, &str)], out: &mut Vec<Option<u32>>) -> bool {
        self.build_query(bank_type, conditions, false, out)
    }

    /// The one query builder: one entry per condition list of `bank_type` in `out`, set for each
    /// known pair. A pair with an unknown tag or value fails the query when `strict` (false, `out`
    /// partly written), else is left out. False, `out` untouched, when the bank type has no names.
    fn build_query(&self, bank_type: u32, conditions: &[(&str, &str)], strict: bool, out: &mut Vec<Option<u32>>) -> bool {
        let Some(bank) = self.banks.get(&bank_type) else { return false };
        out.clear();
        out.resize(bank.conditions.len(), None);
        for (tag, name) in conditions {
            match self.condition_index(bank_type, tag).and_then(|k| Some((k, self.value_at(bank_type, k, name)?))) {
                Some((k, v)) => out[k] = Some(v),
                None if strict => return false,
                None => {}
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bank_xml() {
        let xml = r#"<?xml version="1.0"?>
<dataroot>
  <!-- a comment with <tags> -->
  <sound_bank_event sound_event_name ="SILENT">
  </sound_bank_event>
<!---- CANNON ---->
  <sound_bank_event sound_event_name ="CANNON_Fire_Close">
    <gun_type>cannon</gun_type>
    <gun_type>howitzer</gun_type>
	<audio_distance>close</audio_distance>
  </sound_bank_event>
</dataroot>"#;
        let e = parse_bank_xml(xml);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].event_name, "SILENT");
        assert!(e[0].conditions.is_empty());
        assert_eq!(e[1].conditions, vec![
            ("gun_type".into(), "cannon".into()),
            ("gun_type".into(), "howitzer".into()),
            ("audio_distance".into(), "close".into()),
        ]);
    }

    /// A vocabulary of one bank type (7) with lists `gun_type` (0) and `audio_distance` (2);
    /// list 1 has no tag.
    fn vocabulary() -> BankVocabulary {
        let names = |tag: &str, v: &[(&str, u32)]| Some(ConditionNames { tag: tag.into(), values: v.iter().map(|&(n, k)| (n.into(), k)).collect() });
        let bank = BankNames { source: "sound_bank_test".into(), conditions: vec![names("gun_type", &[("cannon", 4), ("musket", 9)]), None, names("audio_distance", &[("close", 1), ("far", 3)])] };
        BankVocabulary { banks: HashMap::from([(7, bank)]) }
    }

    #[test]
    fn values_are_found_in_any_case() {
        let v = vocabulary();
        assert_eq!(v.value(7, "gun_type", "cannon"), Some(4));
        assert_eq!(v.value(7, "GUN_TYPE", "Musket"), Some(9));
        assert_eq!(v.value(7, "gun_type", "howitzer"), None);
        assert_eq!(v.value(8, "gun_type", "cannon"), None);
    }

    /// The reusable query leaves unknown pairs out where `query` fails, and otherwise matches it.
    #[test]
    fn query_known_into_leaves_unknown_pairs_out() {
        let v = vocabulary();
        let mut q = vec![Some(77); 9];
        assert!(v.query_known_into(7, &[("gun_type", "Cannon"), ("audio_distance", "close")], &mut q));
        assert_eq!(Some(q.clone()), v.query(7, &[("gun_type", "Cannon"), ("audio_distance", "close")]));
        assert!(v.query_known_into(7, &[("gun_type", "musket"), ("shot_type", "bullet"), ("audio_distance", "medium")], &mut q));
        assert_eq!(q, [Some(9), None, None]);
        assert_eq!(v.query(7, &[("gun_type", "musket"), ("audio_distance", "medium")]), None);
        assert!(!v.query_known_into(8, &[("gun_type", "cannon")], &mut q));
        assert_eq!(q, [Some(9), None, None], "untouched for an unknown bank type");
    }
}
