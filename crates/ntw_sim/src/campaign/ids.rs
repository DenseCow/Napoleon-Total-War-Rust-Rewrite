//! Typed ids for campaign objects.
//!
//! The original links campaign objects together with **32-bit ids** stored in the save/startpos
//! ESF (CONFIRMED, ARCHITECTURE_REPORT §11; W3 §3.3–§3.6). Whether those ids are pointers or
//! handles is UNKNOWN, so we simply keep the raw number. Every id wraps exactly the integer type the
//! ESF stores, so loading and re-saving gives back the same bits.
//!
//! A "newtype" (a struct with one field) stops us from mixing up, say, a faction id with a
//! character id: the compiler rejects `FactionId` where a `CharacterId` is expected.
//!
//! All ids derive `Ord`, so they can be keys of a `BTreeMap`. A `BTreeMap` iterates in key order,
//! which keeps the simulation deterministic.

macro_rules! campaign_id {
    ($(#[$meta:meta])* $name:ident, $raw:ty) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $raw);

        impl $name {
            /// Wraps the raw value stored in the ESF.
            pub const fn from_raw(raw: $raw) -> Self {
                $name(raw)
            }

            /// Returns the raw value, exactly as it must be written back to a save.
            pub const fn raw(self) -> $raw {
                self.0
            }
        }
    };
}

campaign_id!(
    /// A faction's object id. W3 §3.3 `FACTION` v18: the leading **i32** "object id", used by
    /// diplomacy and armies to refer to a faction (CONFIRMED structure; e.g. france = 749327284 in
    /// eur). Whether it is a serialized pointer or a handle is INFERRED/UNKNOWN.
    FactionId,
    i32
);

campaign_id!(
    /// A region's id. W3 §3.7 lists `REGION` v5 records; the exact id field is not named in the
    /// report, so `u32` is INFERRED (regions are counted, 72 in eur, and never negative).
    RegionId,
    u32
);

campaign_id!(
    /// A character's id. W3 §3.5 `CHARACTER` = {LOCOMOTABLE, CHARACTER_DETAILS, **i32 id**, ...}
    /// (CONFIRMED structure).
    CharacterId,
    i32
);

campaign_id!(
    /// A military force's id. W3 §3.6 `MILITARY_FORCE` = {**u32 force_id**, u32 commander id, ...}
    /// (CONFIRMED structure).
    ForceId,
    u32
);

campaign_id!(
    /// A unit's id. W3 §3.6 `UNIT` v3 = {..., **i32 unit_id**, u32 men, u32 max_men, ...}
    /// (CONFIRMED structure).
    UnitId,
    i32
);

campaign_id!(
    /// A fort's id. `REGION/FORT_ARRAY[]`'s leading u32 (the one field the region's reader
    /// `0x00A51E30` reads itself, CONFIRMED); **what the number identifies is UNKNOWN** -- the
    /// loader hands it straight to the exe's id-to-object lookup (`0x0105AC60`) as the fort's
    /// object, so it is an id in the model's global id space like every other object
    /// (SAVE_COMPAT.md §3), not necessarily the fort's own. 0 when the record has none.
    FortId,
    u32
);

campaign_id!(
    /// A recruitment queue item's id: the inner `RECRUITMENT_ITEM` #0 i32 (pointer-like and unique
    /// in the original's saves, one global id space with every other object: SAVE_COMPAT.md §3).
    /// Unique across the world: the loader (`ntw_campaign`) renumbers a missing or repeated file
    /// id, and the model's one store site (the recruit command) takes a fresh
    /// [`World::alloc_id`](super::world::World::alloc_id), unique by construction.
    /// The commands and the UI address a queued item by it, so an item keeps its identity while the
    /// queue around it changes.
    RecruitmentItemId,
    i32
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_raw_values() {
        // The france id from W3 §3.3 (a structural fact, not gameplay data).
        assert_eq!(FactionId::from_raw(749_327_284).raw(), 749_327_284);
        assert_eq!(FactionId::from_raw(-1).raw(), -1);
        assert_eq!(RegionId::from_raw(u32::MAX).raw(), u32::MAX);
        assert_eq!(CharacterId::from_raw(i32::MIN).raw(), i32::MIN);
        assert_eq!(ForceId::from_raw(7).raw(), 7);
        assert_eq!(UnitId::from_raw(-5).raw(), -5);
        assert_eq!(FortId::from_raw(11).raw(), 11);
    }

    #[test]
    fn ids_order_by_raw_value() {
        assert!(FactionId(-3) < FactionId(2));
        assert!(RegionId(1) < RegionId(10));
    }
}
