//! The state mods' rules keep in a campaign (DESIGN.md §3.3.1): values the original game has no
//! field for (a mod's famine counter per region, a new resource per faction, ...), saved in our own
//! saves and hashed like the rest of the model.
//!
//! Deterministic by construction: the values are kept in a `BTreeMap` by [`ModKey`] (owner, scope,
//! name), so they are iterated, hashed and saved in key order whatever order they were written in.
//! Nothing here is capped: a mod keeps as many values as it needs.
//!
//! A rule never writes the model directly: it reads [`ModState`] through the model and returns its
//! changes in a [`ModWrites`], which the caller applies in order ([`ModState::apply`]) when the rule
//! runs for real, and drops when it runs for a projection (a panel's preview never changes state).
//! Vanilla rules write nothing; an empty [`ModWrites`] allocates nothing.

use std::collections::BTreeMap;

use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use crate::fnv::Fnv64;

/// What a mod value belongs to. A value of an entity that is gone stays until its mod removes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ModScope {
    /// The whole campaign.
    Campaign,
    /// A faction.
    Faction(FactionId),
    /// A region.
    Region(RegionId),
    /// A character.
    Character(CharacterId),
    /// An army or navy.
    Force(ForceId),
}

/// The key of one mod value: the mod (or rule implementation) that owns it, what it belongs to and
/// its name. Ordered by owner, then scope, then name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ModKey {
    /// The owner, e.g. the rule implementation's key.
    pub owner: String,
    /// What the value belongs to.
    pub scope: ModScope,
    /// The value's name within its owner and scope.
    pub name: String,
}

impl ModKey {
    /// A key from its parts.
    pub fn new(owner: &str, scope: ModScope, name: &str) -> Self {
        ModKey { owner: owner.to_owned(), scope, name: name.to_owned() }
    }
}

/// One mod value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ModValue {
    /// A flag.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A number (saved and hashed bit for bit).
    Float(f64),
    /// A text, e.g. a key.
    Text(String),
    /// A list, in order.
    List(Vec<ModValue>),
}

impl ModValue {
    fn hash_into(&self, h: &mut Fnv64) {
        match self {
            ModValue::Bool(b) => {
                h.u32(0);
                h.u32(u32::from(*b));
            }
            ModValue::Int(i) => {
                h.u32(1);
                h.bytes(&i.to_le_bytes());
            }
            ModValue::Float(x) => {
                h.u32(2);
                h.bytes(&x.to_bits().to_le_bytes());
            }
            ModValue::Text(s) => {
                h.u32(3);
                h.str(s);
            }
            ModValue::List(items) => {
                h.u32(4);
                h.u32(items.len() as u32);
                for v in items {
                    v.hash_into(h);
                }
            }
        }
    }
}

/// The mods' values in a campaign (see the module docs). Saved with the model.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ModState {
    values: BTreeMap<ModKey, ModValue>,
}

impl ModState {
    /// The value at `key`.
    pub fn get(&self, key: &ModKey) -> Option<&ModValue> {
        self.values.get(key)
    }

    /// Sets the value at `key`.
    pub fn set(&mut self, key: ModKey, value: ModValue) {
        self.values.insert(key, value);
    }

    /// Removes the value at `key`, giving it back.
    pub fn remove(&mut self, key: &ModKey) -> Option<ModValue> {
        self.values.remove(key)
    }

    /// Every value in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ModKey, &ModValue)> {
        self.values.iter()
    }

    /// How many values there are.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// There are no values (always so in vanilla).
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Applies a rule's writes in the order it made them, leaving `writes` empty.
    pub fn apply(&mut self, writes: &mut ModWrites) {
        for (key, value) in writes.0.drain(..) {
            match value {
                Some(v) => self.set(key, v),
                None => {
                    self.remove(&key);
                }
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut Fnv64) {
        h.u32(self.values.len() as u32);
        for (k, v) in &self.values {
            h.str(&k.owner);
            match k.scope {
                ModScope::Campaign => h.u32(0),
                ModScope::Faction(id) => {
                    h.u32(1);
                    h.i32(id.raw());
                }
                ModScope::Region(id) => {
                    h.u32(2);
                    h.u32(id.raw());
                }
                ModScope::Character(id) => {
                    h.u32(3);
                    h.i32(id.raw());
                }
                ModScope::Force(id) => {
                    h.u32(4);
                    h.u32(id.raw());
                }
            }
            h.str(&k.name);
            v.hash_into(h);
        }
    }
}

/// The changes a rule makes to [`ModState`], in order (see the module docs).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModWrites(Vec<(ModKey, Option<ModValue>)>);

impl ModWrites {
    /// Sets the value at `key`.
    pub fn set(&mut self, key: ModKey, value: ModValue) {
        self.0.push((key, Some(value)));
    }

    /// Removes the value at `key`.
    pub fn remove(&mut self, key: ModKey) {
        self.0.push((key, None));
    }

    /// No changes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
