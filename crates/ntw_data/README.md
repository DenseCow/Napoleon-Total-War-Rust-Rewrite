# ntw_data

Typed game records built from the original game's database tables. The tables are read
from the player's own install, read-only, through `ntw_formats`. No Creative Assembly data is
included. The only numbers in this crate are the made-up placeholders in `GameDatabase::test_fixture()`.

## What it gives you

```rust
use ntw_data::GameDatabase;

let db = GameDatabase::from_install(r"C:\...\Napoleon Total War\data")?;
let unit = db.land_unit("Inf_Line_Austrian_German_Fusiliers").unwrap();
println!("{} men, accuracy {}", unit.stats.num_men, unit.stats.accuracy);   // 160 men, accuracy 40
println!("fires {:?}", unit.projectile.map(|p| &p.key));                     // musket_flintlock
let morale_rules = &db.kv_morale;   // ntw_sim::battle::morale::KvMorale, ready for the simulation
```

No install? `GameDatabase::test_fixture()` has a few units, all keyed `fixture_*`, with obviously made-up numbers.

## Modules

- **`schemas`**: one struct per table. Fields are in file order and each has a doc comment
  `#column @exe-offset confidence`. Columns whose meaning is unknown are named `unknown_<offset>`.
  - `UnitRecord` (units, v4, 25 columns), `UnitStatsLand` (unit_stats_land, v5, 89 columns),
    `Projectile` (projectiles, v1, 35 columns), `GunTypeProjectile`;
  - `FactionRecord`, `RegionRecord`, `BuildingLevel`, `Technology`.

  The column order, types and version guards come from the exe's own row readers
  (`analysis/worker1/DB_BUILDERS.md`). The int/float split and the names come from
  Workers 2 and 3. **All names are inferred**, since the files contain none.
- **`kv`**: the key-value tables. The exe converts most kv values to integers by
  *truncating* (`cvttss2si`: 2.9 becomes 2, -2.9 becomes -2). `exe_truncate` copies that exactly.
  `_kv_morale` and `_kv_fatigue` become `ntw_sim`'s `KvMorale` and `KvFatigue` (the field names
  are the kv keys). `_kv_rules` is read per key with the exe's int/float choice (`KV_RULES_KEYS`) and also
  becomes `ntw_sim`'s `KvRules` (`GameDatabase::kv_rules_sim`).
- **`record`**: `Table<T>` (rows plus a key index) and the `db_record!` macro. One field list
  generates the struct, its binary schema and the row decoder, so they always agree.
- **`GameDatabase`**: loads all of the above **eagerly** (well under 1 MB, a few milliseconds)
  and offers `unit`, `unit_stats`, `projectile`, `faction`, `region`, `building_level` and
  `technology`. It also follows foreign keys: `land_unit` (unit → stats → projectile) and
  `gun_projectiles` (artillery gun type → projectiles).

## Tests
- `cargo test -p ntw_data` runs the unit tests: fixture lookups, the schema layouts checked
  letter by letter against the exe, version guards and kv truncation.
- `cargo test -p ntw_data -- --ignored --nocapture` runs the real-install tests (read-only). They
  check that every table decodes with no leftover bytes and that the row counts and versions are
  right. They also spot-check values (Austrian fusiliers: 160 men, accuracy 40, melee 6; France's
  colours; Paris region colour) and print the kv truncation effects.
