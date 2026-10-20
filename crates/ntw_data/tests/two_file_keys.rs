//! A decode and merge smoke test of every typed table across two files: each record type decodes
//! from a vanilla file and a mod file, a mod row repeating the vanilla row merges into it (one row)
//! and a row with other text is added. It does NOT check that each table's `key()` is the key the
//! exe merges by: every text column differs between the two sample rows, so any `key()` passes.
//! `same_key_in_two_files_...` checks which row survives (row rule, bob_, pack priority).
//! (The raw tables and `merged_rows` are covered in `ntw_formats` db_folder.rs.)

use std::path::PathBuf;

use ntw_data::{DbRecord, load_table};
use ntw_formats::db::{DbTable, DbValue, FieldType};
use ntw_formats::pack::{PackFile, Vfs};

fn temp_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ntw_data_keys_{}_{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A PFH0 pack with the given type and files.
fn pack(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    pack_of_type(1, files)
}

/// A PFH0 pack with the given header type (1 release, 3 mod) and files.
fn pack_of_type(pack_type: u32, files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut index = Vec::new();
    for (path, data) in files {
        index.extend_from_slice(&(data.len() as u32).to_le_bytes());
        index.extend_from_slice(path.as_bytes());
        index.push(0);
    }
    let mut b = b"PFH0".to_vec();
    for v in [pack_type, 0, 0, files.len() as u32, index.len() as u32] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&index);
    for (_, data) in files {
        b.extend_from_slice(data);
    }
    b
}

/// A row whose text columns are all `tag` + the column number (so two tags give two different
/// keys whichever column `key()` reads: this proves nothing about the right key); others default.
fn row<T: DbRecord>(tag: &str) -> Vec<DbValue> {
    T::schema()
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| match f.ty {
            FieldType::Str => DbValue::Str(format!("{tag}{i}")),
            FieldType::OptStr => DbValue::OptStr(None),
            FieldType::Bool => DbValue::Bool(false),
            FieldType::I32 => DbValue::I32(0),
            FieldType::F32 => DbValue::F32(0.0),
            FieldType::U16 => DbValue::U16(0),
        })
        .collect()
}

fn two_files<T: DbRecord>() {
    let schema = T::schema();
    // A version above every column's `since`, so all columns are present.
    let encode = |rows: Vec<Vec<DbValue>>| DbTable { version: 99, has_version_marker: true, flag: 1, rows }.to_bytes(&schema).unwrap();
    let (a, b) = (row::<T>("a"), row::<T>("b"));
    let key_of = |r: &[DbValue]| T::from_row(r).unwrap_or_else(|e| panic!("{}: column {e}", T::TABLE)).key().to_owned();
    let (key_a, key_b) = (key_of(&a), key_of(&b));
    assert_ne!(key_a, key_b, "{}: the sample rows share a key", T::TABLE);
    let path = |f: &str| format!("db\\{}_tables\\{f}", T::TABLE);
    let dir = temp_dir(T::TABLE);
    std::fs::write(dir.join("rel.pack"), pack(&[(&path(T::TABLE), encode(vec![a.clone()]))])).unwrap();
    std::fs::write(dir.join("mod.pack"), pack(&[(&path("more"), encode(vec![a, b]))])).unwrap();
    let mut vfs = Vfs::new();
    vfs.mount(PackFile::open(dir.join("rel.pack")).unwrap());
    vfs.mount(PackFile::open(dir.join("mod.pack")).unwrap());
    let mut warnings = Vec::new();
    let table = load_table::<T>(&vfs, &mut warnings).unwrap_or_else(|e| panic!("{}: {e}", T::TABLE));
    assert!(warnings.is_empty(), "{}: {warnings:?}", T::TABLE);
    assert_eq!(table.rows().len(), 2, "{}: the repeated row merges, the new key is added", T::TABLE);
    assert!(table.get(&key_a).is_some() && table.get(&key_b).is_some(), "{}", T::TABLE);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_typed_table_decodes_and_merges_across_two_files() {
    two_files::<ntw_data::campaign::CampaignVariable>();
    two_files::<ntw_data::campaign::CampaignVariableOverride>();
    two_files::<ntw_data::campaign::BuildingEffect>();
    two_files::<ntw_data::campaign::BuildingUnitAllowed>();
    two_files::<ntw_data::campaign::BuildingUpgrade>();
    two_files::<ntw_data::campaign::BuildingChain>();
    two_files::<ntw_data::campaign::BuildingChainSlot>();
    two_files::<ntw_data::campaign::TaxLevel>();
    two_files::<ntw_data::campaign::TaxKey>();
    two_files::<ntw_data::campaign::TaxEffect>();
    two_files::<ntw_data::campaign::UnitTechnologyRecord>();
    two_files::<ntw_data::campaign::BuildingTechnologyRecord>();
    two_files::<ntw_data::campaign::BuildingFactionVariant>();
    two_files::<ntw_data::campaign::BuildingCultureVariant>();
    two_files::<ntw_data::campaign::TechnologyRequirementRecord>();
    two_files::<ntw_data::campaign::TechnologyFactionRecord>();
    two_files::<ntw_data::campaign::GovernmentTypeRecord>();
    two_files::<ntw_data::campaign::GovernmentEffect>();
    two_files::<ntw_data::campaign::AgentRecord>();
    two_files::<ntw_data::campaign::UnitFactionPermission>();
    two_files::<ntw_data::campaign::UnitGovernmentPermission>();
    two_files::<ntw_data::campaign::MapSlotRecord>();
    two_files::<ntw_data::campaign::MapTownRecord>();
    two_files::<ntw_data::campaign::SlotArtRecord>();
    two_files::<ntw_data::campaign::SlotTemplateModelRecord>();
    two_files::<ntw_data::campaign::TradeNodeRecord>();
    two_files::<ntw_data::campaign::ReligionRelationRecord>();
    two_files::<ntw_data::campaign::CommodityDemandRecord>();
    two_files::<ntw_data::campaign::GovernmentRelationRecord>();
    two_files::<ntw_data::campaign::NegotiationStringRecord>();
    two_files::<ntw_data::campaign::NegotiationOverrideStringRecord>();
    two_files::<ntw_data::campaign::AttitudeThresholdRecord>();
    two_files::<ntw_data::campaign::BuildingChainRecord>();
    two_files::<ntw_data::campaign::NavalStatsRecord>();
    two_files::<ntw_data::campaign::CampaignGroundType>();
    two_files::<ntw_data::campaign::AgentCultureDetail>();
    two_files::<ntw_data::campaign::HistoricalCharacter>();
    two_files::<ntw_data::characters::CharacterTraitRecord>();
    two_files::<ntw_data::characters::TraitInfo>();
    two_files::<ntw_data::characters::TraitAntitrait>();
    two_files::<ntw_data::characters::TraitIncludedAgent>();
    two_files::<ntw_data::characters::AncillaryRecord>();
    two_files::<ntw_data::characters::AncillaryIncludedAgent>();
    two_files::<ntw_data::characters::AncillaryExcluded>();
    two_files::<ntw_data::characters::AncillarySubculture>();
    two_files::<ntw_data::characters::SubcultureRecord>();
    two_files::<ntw_data::characters::CultureRecord>();
    two_files::<ntw_data::events::EventRecord>();
    two_files::<ntw_data::events::EventViewGroup>();
    two_files::<ntw_data::events::EventEffectGroup>();
    two_files::<ntw_data::events::EventHistoricalCharacter>();
    two_files::<ntw_data::events::EventPolicy>();
    two_files::<ntw_data::events::GroupingRecord>();
    two_files::<ntw_data::events::PolicyRecord>();
    two_files::<ntw_data::events::GroupingFaction>();
    two_files::<ntw_data::events::GroupingCulture>();
    two_files::<ntw_data::events::GroupingSubculture>();
    two_files::<ntw_data::events::GroupingRegion>();
    two_files::<ntw_data::events::GroupingContinent>();
    two_files::<ntw_data::events::GroupingEmpire>();
    two_files::<ntw_data::characters::AgentAttributeRecord>();
    two_files::<ntw_data::effects::EffectBonusBasic>();
    two_files::<ntw_data::effects::EffectBonusUnitCategory>();
    two_files::<ntw_data::effects::EffectBonusUnitClass>();
    two_files::<ntw_data::effects::EffectBonusPopClass>();
    two_files::<ntw_data::effects::EffectBonusReligion>();
    two_files::<ntw_data::effects::EffectBonusPopClassReligion>();
    two_files::<ntw_data::effects::EffectBonusChain>();
    two_files::<ntw_data::effects::ReligionConversionMod>();
    two_files::<ntw_data::effects::EffectBonusAgent>();
    two_files::<ntw_data::effects::TechnologyEffect>();
    two_files::<ntw_data::effects::BuildingFactionwideEffect>();
    two_files::<ntw_data::effects::TraitLevel>();
    two_files::<ntw_data::effects::TraitLevelEffect>();
    two_files::<ntw_data::effects::TraitAttributeEffect>();
    two_files::<ntw_data::effects::AncillaryAttributeEffect>();
    two_files::<ntw_data::effects::AncillaryEffect>();
    two_files::<ntw_data::effects::MinisterialEffect>();
    two_files::<ntw_data::effects::MinisterialEffectiveness>();
    two_files::<ntw_data::effects::DifficultyHandicapEffect>();
    two_files::<ntw_data::schemas::UnitRecord>();
    two_files::<ntw_data::schemas::UnitStatsLand>();
    two_files::<ntw_data::schemas::Projectile>();
    two_files::<ntw_data::schemas::GunTypeProjectile>();
    two_files::<ntw_data::schemas::FactionRecord>();
    two_files::<ntw_data::schemas::RegionRecord>();
    two_files::<ntw_data::schemas::CampaignMapPlayableArea>();
    two_files::<ntw_data::schemas::SlotTypeRecord>();
    two_files::<ntw_data::schemas::BuildingLevel>();
    two_files::<ntw_data::schemas::Technology>();
    two_files::<ntw_data::schemas::UnitToUnitAbility>();
    two_files::<ntw_data::schemas::UnitClassToUnitAbility>();
    two_files::<ntw_data::schemas::TechnologyEffect>();
    two_files::<ntw_data::schemas::EffectUnitAbility>();
    two_files::<ntw_data::schemas::EffectShotType>();
    two_files::<ntw_data::schemas::BattleWeatherType>();
    two_files::<ntw_data::schemas::BattleClimateWeather>();
    two_files::<ntw_data::schemas::UnitStatsLandExperienceBonuses>();
    two_files::<ntw_data::schemas::UnitStatsNavalExperienceBonuses>();
    two_files::<ntw_data::schemas::FatigueEffect>();
}

/// One `campaign_variables` row `(key, value)` per entry, as a table file.
fn variables_file(rows: &[(&str, f32)]) -> Vec<u8> {
    let rows = rows.iter().map(|(k, v)| vec![DbValue::Str((*k).to_owned()), DbValue::F32(*v)]).collect();
    DbTable { version: 99, has_version_marker: true, flag: 1, rows }.to_bytes(&ntw_data::campaign::CampaignVariable::schema()).unwrap()
}

/// A pack to mount: its header type, the table file name and the rows `(key, value)`.
type VariablesPack<'a> = (u32, &'a str, &'a [(&'a str, f32)]);

/// Loads `campaign_variables` from `VariablesPack`s mounted in this order,
/// and returns the values of keys `k` and `other`, plus the row count.
fn load_variables(test: &str, packs: &[VariablesPack]) -> (Option<f32>, Option<f32>, usize) {
    let dir = temp_dir(test);
    let mut vfs = Vfs::new();
    for (i, (ty, file, rows)) in packs.iter().enumerate() {
        let path = format!("db\\campaign_variables_tables\\{file}");
        let name = dir.join(format!("p{i}.pack"));
        std::fs::write(&name, pack_of_type(*ty, &[(&path, variables_file(rows))])).unwrap();
        vfs.mount(PackFile::open(name).unwrap());
    }
    let mut warnings = Vec::new();
    let table = load_table::<ntw_data::campaign::CampaignVariable>(&vfs, &mut warnings).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = std::fs::remove_dir_all(&dir);
    (table.get("k").map(|r| r.value), table.get("other").map(|r| r.value), table.len())
}

/// Which row survives when two files hold the same key with different values
/// (`DB_LoadTableFolderMergedByKey` 0x00E778A0 row rule, `Vfs::db_row_replaces`): at equal
/// priority the first stays unless the new file is named `bob_*`; a higher-priority pack replaces.
#[test]
fn same_key_in_two_files_keeps_the_row_the_original_keeps() {
    let (base, extra) = (&[("k", 1.0)][..], &[("k", 2.0), ("other", 3.0)][..]);
    // Equal priority, plain name: the first row stays, the new key is added.
    assert_eq!(load_variables("plain", &[(1, "campaign_variables", base), (1, "more", extra)]), (Some(1.0), Some(3.0), 2));
    // Equal priority, `bob_` name: the later row replaces the first, in its place.
    assert_eq!(load_variables("bob", &[(1, "campaign_variables", base), (1, "bob_more", extra)]), (Some(2.0), Some(3.0), 2));
    // A mod pack (higher priority) replaces the vanilla row whatever the file name.
    assert_eq!(load_variables("mod", &[(1, "campaign_variables", base), (3, "more", extra)]), (Some(2.0), Some(3.0), 2));
    // A lower-priority pack mounted later never replaces, even with a `bob_` name.
    assert_eq!(load_variables("lower", &[(3, "campaign_variables", base), (1, "bob_more", extra)]), (Some(1.0), Some(3.0), 2));
}
