//! The campaign scripts' `save_value` slots in a save: `CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS`
//! v17, child `LUA[]` (CONFIRMED in the user's saves: one value per item, in `save_value` call
//! order).
//!
//! Types (CONFIRMED from the exe's Lua handlers, `analysis/campaign/CAMPAIGN_DATA.md` §4):
//! `save_value(v, context)` (`FUN_009796c0`) writes a **bool** (ESF 0x01) when `v` is a Lua
//! boolean and otherwise `lua_tointeger(v)` as an **i32** (ESF 0x04): numbers lose their fraction
//! and strings that are not numbers become 0. `load_value(default, context)` (`FUN_009797f0`)
//! reads the next item with the type of `default` (bool or integer) and returns the default when
//! no items are left.

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord, EsfRecordArray};

/// One stored value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScriptSaveValue {
    /// A Lua boolean.
    Bool(bool),
    /// Anything else, as `lua_tointeger` gives it.
    Int(i32),
}

fn episodic(root: &EsfRecord) -> Option<&EsfRecord> {
    root.child("CAMPAIGN_ENV")?.child("CAMPAIGN_MODEL")?.child("EPISODIC_RESTRICTIONS")
}

/// The stored values of a save or start position, in order (empty if there are none).
pub fn read_script_values(esf: &EsfFile) -> Vec<ScriptSaveValue> {
    let Some(lua) = episodic(&esf.root).and_then(|e| e.record_array("LUA")) else { return Vec::new() };
    lua.items
        .iter()
        .filter_map(|it| match it.first()? {
            EsfNode::Bool(b) => Some(ScriptSaveValue::Bool(*b)),
            n => n.as_int().map(|v| ScriptSaveValue::Int(v as i32)),
        })
        .collect()
}

/// Replaces the `LUA[]` items of `esf` with `values`. Returns false if the file has no
/// `EPISODIC_RESTRICTIONS/LUA` array (nothing is written then).
pub fn write_script_values(esf: &mut EsfFile, values: &[ScriptSaveValue]) -> bool {
    let Some(lua) = episodic_lua_mut(&mut esf.root) else { return false };
    lua.items = values
        .iter()
        .map(|v| {
            vec![match *v {
                ScriptSaveValue::Bool(b) => EsfNode::Bool(b),
                ScriptSaveValue::Int(i) => EsfNode::I32(i),
            }]
        })
        .collect();
    true
}

fn episodic_lua_mut(root: &mut EsfRecord) -> Option<&mut EsfRecordArray> {
    episodic_array_mut(root, "LUA")
}

fn episodic_array_mut<'a>(root: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecordArray> {
    let env = child_mut(root, "CAMPAIGN_ENV")?;
    let model = child_mut(env, "CAMPAIGN_MODEL")?;
    let ep = child_mut(model, "EPISODIC_RESTRICTIONS")?;
    ep.children.iter_mut().find_map(|c| match c {
        EsfNode::RecordArray(a) if a.name == name => Some(&mut **a),
        _ => None,
    })
}

/// The scripts' restriction lists of a save: `EPISODIC_RESTRICTIONS/BUILDING_RESTRICTIONS[]`
/// (the keys given to `add_restricted_building_level_record`; CONFIRMED in the user's vanilla
/// `auto_save.save`: 55 items of one utf16 string each, the start position's list is empty) and
/// `UNIT_RESTRICTIONS[]` (`add_restricted_unit_record`; INFERRED the same shape, every vanilla
/// list seen is empty). Without them a loaded game offers the tutorial and Peninsular buildings
/// again, because the scripts restrict them only on a new game.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptRestrictions {
    /// Building level keys, in list order.
    pub buildings: Vec<String>,
    /// Unit keys, in list order.
    pub units: Vec<String>,
}

/// The restriction lists of a save or start position (empty lists if there are none).
pub fn read_restrictions(esf: &EsfFile) -> ScriptRestrictions {
    let keys = |name: &str| -> Vec<String> {
        episodic(&esf.root)
            .and_then(|e| e.record_array(name))
            .map(|a| a.items.iter().filter_map(|it| it.first().and_then(EsfNode::as_str).map(str::to_string)).collect())
            .unwrap_or_default()
    };
    ScriptRestrictions { buildings: keys("BUILDING_RESTRICTIONS"), units: keys("UNIT_RESTRICTIONS") }
}

/// Replaces both restriction lists of `esf`. Returns false if the file has no
/// `EPISODIC_RESTRICTIONS` lists (nothing is written then).
pub fn write_restrictions(esf: &mut EsfFile, r: &ScriptRestrictions) -> bool {
    let mut ok = true;
    for (name, keys) in [("BUILDING_RESTRICTIONS", &r.buildings), ("UNIT_RESTRICTIONS", &r.units)] {
        match episodic_array_mut(&mut esf.root, name) {
            Some(a) => a.items = keys.iter().map(|k| vec![EsfNode::Utf16String(k.clone())]).collect(),
            None => ok = false,
        }
    }
    ok
}

fn child_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    r.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(b) if b.name == name => Some(&mut **b),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with_lua(items: Vec<Vec<EsfNode>>) -> EsfFile {
        let mut lua = EsfRecordArray::new("LUA", 0);
        lua.items = items;
        let mut ep = EsfRecord::new("EPISODIC_RESTRICTIONS", 17);
        ep.children.push(EsfNode::RecordArray(Box::new(lua)));
        let mut model = EsfRecord::new("CAMPAIGN_MODEL", 10);
        model.children.push(EsfNode::Record(Box::new(ep)));
        let mut env = EsfRecord::new("CAMPAIGN_ENV", 2);
        env.children.push(EsfNode::Record(Box::new(model)));
        let mut root = EsfRecord::new("CAMPAIGN_SAVE_GAME", 5);
        root.children.push(EsfNode::Record(Box::new(env)));
        EsfFile::new(root)
    }

    #[test]
    fn read_and_write_round_trip() {
        let mut f = file_with_lua(vec![vec![EsfNode::I32(3)], vec![EsfNode::Bool(true)]]);
        assert_eq!(read_script_values(&f), vec![ScriptSaveValue::Int(3), ScriptSaveValue::Bool(true)]);
        let vals = [ScriptSaveValue::Bool(false), ScriptSaveValue::Int(-7), ScriptSaveValue::Int(2)];
        assert!(write_script_values(&mut f, &vals));
        let back = EsfFile::from_bytes(&f.to_bytes().unwrap()).unwrap();
        assert_eq!(read_script_values(&back), vals);
        assert!(!write_script_values(&mut EsfFile::new(EsfRecord::new("X", 0)), &vals));
    }

    #[test]
    fn restriction_lists_round_trip() {
        let mut f = file_with_lua(Vec::new());
        // The start position's shape: both lists present and empty.
        for name in ["UNIT_RESTRICTIONS", "BUILDING_RESTRICTIONS"] {
            let env = child_mut(&mut f.root, "CAMPAIGN_ENV").unwrap();
            let model = child_mut(env, "CAMPAIGN_MODEL").unwrap();
            let ep = child_mut(model, "EPISODIC_RESTRICTIONS").unwrap();
            ep.children.insert(0, EsfNode::RecordArray(Box::new(EsfRecordArray::new(name, 0))));
        }
        assert_eq!(read_restrictions(&f), ScriptRestrictions::default());
        let r = ScriptRestrictions { buildings: vec!["rIronTutorial1_iron_mine".into(), "pTradeTutorial1_trading_port".into()], units: vec!["Inf_Line_Spanish".into()] };
        assert!(write_restrictions(&mut f, &r));
        let back = EsfFile::from_bytes(&f.to_bytes().unwrap()).unwrap();
        assert_eq!(read_restrictions(&back), r);
        assert!(!write_restrictions(&mut EsfFile::new(EsfRecord::new("X", 0)), &r));
    }
}
