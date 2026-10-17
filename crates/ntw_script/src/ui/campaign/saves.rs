//! The campaign's save and load functions: the escape menu's save / load panel
//! (`load-save_game.luac`), the quick save / load shortcuts and `CanSave` (UI_FIDELITY.md §5.1;
//! CONFIRMED unless tagged).
//!
//! The panel calls the same listing and loading names on `CampaignUI` as the front end's load page
//! does on `FrontEnd` (`FileExtenstionAndPathForWriteClass`, `EnumerateCampaignSaves`,
//! `GetExtendedSaveGameInfo`, `LoadCampaign`); those come from one implementation,
//! [`crate::ui::frontend`]'s `install_saves`. What only the campaign has is here: writing a save
//! ([`super::CampaignSaves`], the game's synchronous callback) and naming it.

use std::path::PathBuf;
use std::rc::Rc;

use mlua::{Lua, Table, Value, Variadic};

use super::super::frontend::{SaveFolders, install_saves, is_valid_save_name, latest_campaign_save, save_info_table, write_class};
use super::super::host::{Inner, UiRequest, log};
use super::{CampaignUi, loc, loc_required, round_description};

/// Where a save name goes.
struct SaveTarget {
    /// The file written: `<our user folder>\save_games\<file>`.
    path: PathBuf,
    /// The save class's folders, ours and the original's: the ones the panel lists.
    folders: SaveFolders,
    /// The file name, extension included.
    file: String,
}

impl SaveTarget {
    /// A save of this name is already listed (ours or the original's, the folders the panel's
    /// `EnumerateCampaignSaves` lists, [`SaveFolders::find`]): the exe's exists test (`fopen "rbS"`,
    /// `0x004C4D80`) on the one folder it lists.
    fn exists(&self) -> bool {
        self.folders.find(&self.file).is_some()
    }
}

/// The file a save name writes: `<our user folder>\save_games\<name>`, plus `.save` unless the name
/// already ends in it. CONFIRMED rule (`0x00DB7FC0`, write class 0 = save_game; the ending is
/// compared case-sensitively, `0x004F21E0`). The exe checks nothing else; ours refuses a name the
/// panel's field could not have typed ([`is_valid_save_name`], our guard: no separator or `:` can
/// lead out of the folder), and writes only NapoleonRust's own user folder, never the original's.
/// None, logged, when the name is refused or there is no user folder.
fn save_target(inner: &Inner, name: &str) -> Option<SaveTarget> {
    let (ext, folder) = write_class("save_game")?;
    if name.is_empty() || !is_valid_save_name(name) {
        log(inner, format!("ERROR SaveCampaign: refused save name {name:?}"));
        return None;
    }
    let folders = SaveFolders::of(&inner.facts, folder);
    let Some(dir) = folders.ours.as_ref() else {
        log(inner, format!("ERROR SaveCampaign {name}: no NapoleonRust user folder to save into"));
        return None;
    };
    let file = if name.ends_with(ext) { name.to_owned() } else { format!("{name}{ext}") };
    Some(SaveTarget { path: dir.join(&file), folders, file })
}

/// The save itself, `0x00A1F000(path, 0)`: false at once when the campaign's scripts switched saving
/// off (`[+0xFA8]+0x115`, [`ntw_sim::campaign::World::saving_disabled`]), else the game's write. Every
/// failure is logged here (an `ERROR` line, which the game prints; a save the scripts switched off is
/// not an error).
fn write_save(inner: &Inner, ui: &CampaignUi, path: Option<PathBuf>) -> bool {
    let Some(path) = path else { return false };
    if ui.model().world.saving_disabled {
        log(inner, format!("SaveCampaign {}: saving is disabled by the campaign script", path.display()));
        return false;
    }
    let Some(saves) = ui.link.saves.as_ref() else {
        log(inner, format!("ERROR SaveCampaign {}: no game to save", path.display()));
        return false;
    };
    match saves.save(&path) {
        Ok(()) => true,
        Err(e) => {
            log(inner, format!("ERROR SaveCampaign {}: {e}", path.display()));
            false
        }
    }
}

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    install_saves(lua, inner, t)?;

    // CanSave() (`0x009E0D50` → `0x008BE4A0`): false while the campaign's scripts have saving
    // switched off (`disable_saving_game`, CONFIRMED: `0x008BE4A0` tests the same flag the save
    // tests). PROVISIONAL: `0x008BE4A0` tests more campaign fields whose meaning is not traced
    // (byte +0xFE9, `[+0xF60]+0x24` against `+0x28`, `[+0xF6C]+0x58` with `0x008C1970`, `+0xF70`
    // set, faction +0x93E, `[+0xF5C]+0x18` / `+0x3C`), so ours is true otherwise.
    f!("CanSave", |_l, inner, ui, _a: Variadic<Value>| Ok(!ui.model().world.saving_disabled));

    // SaveCampaign(name, overwrite) (`0x009F4A20`; exe help: "Out: true if overwrite request is
    // requested and needed. false, save success if overwrite not requested or not needed"). The
    // overwrite flag is read only when the last argument is a boolean.
    //   - overwrite false and the file exists: one value, true (nothing saved; the panel asks);
    //   - overwrite false, no such file: two values, false and the save's success;
    //   - overwrite true: no exists check, one value, the save's success.
    // "Exists" looks in both folders the panel lists ([`SaveTarget::exists`]), so a name the list
    // shows from the original's folder asks too; the save still goes to our folder, where it then
    // takes that name's place in the list.
    f!("SaveCampaign", |_l, inner, ui, args: Variadic<Value>| {
        let name = match args.first() {
            Some(Value::String(s)) => s.to_string_lossy(),
            _ => String::new(),
        };
        let overwrite = matches!(args.last(), Some(Value::Boolean(true)));
        let target = save_target(&inner, &name);
        if !overwrite && target.as_ref().is_some_and(SaveTarget::exists) {
            return Ok(Variadic::from(vec![true]));
        }
        let saved = write_save(&inner, &ui, target.map(|t| t.path));
        Ok(Variadic::from(if overwrite { vec![saved] } else { vec![false, saved] }))
    });

    // DefaultSaveName() (`0x009E22C0` → `0x009D5400`): "<name> <round> <year>", the round being the
    // calendar's round description (`0x008D1E20`, the one `UpdateFactionFundsAndDate` sends). The
    // name is the campaign's on-screen name (`campaigns_onscreen_name_<key>`) when the loc has it and
    // the campaign's rules say so (`CampaignFeatures::save_name_uses_campaign_name`: the exe's four
    // Napoleon campaigns), else the human faction's display name (faction +0x518, the save's
    // `FACTION` plain value 2). An empty round (a calendar without turn names) keeps both spaces, as
    // the exe's "%S %d" then "%S %S" do.
    f!("DefaultSaveName", |_l, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        let key = ui.link.campaign.as_str();
        let campaign_name = if m.rules.features.save_name_uses_campaign_name { loc(&inner, &format!("campaigns_onscreen_name_{key}")) } else { None };
        let name = campaign_name.unwrap_or_else(|| {
            let human = &ui.link.human;
            match m.faction_by_key(human) {
                Some(f) => super::faction_display_name(&inner, &ui.link.db, &m, f.id, human),
                None => super::faction_name(&inner, &ui.link.db, human),
            }
        });
        let round = round_description(&inner, &m.calendar);
        Ok(format!("{name} {round} {}", m.calendar.date.year))
    });

    // GetCurrentGameInfo() (`0x009E4BB0`): the running campaign's save header (built as a save
    // would build it), as the same details table `GetExtendedSaveGameInfo` gives for a save.
    f!("GetCurrentGameInfo", |lua, inner, ui, _a: Variadic<Value>| {
        let Some(saves) = ui.link.saves.as_ref() else {
            log(&inner, "ERROR GetCurrentGameInfo: no game to describe".into());
            return Ok(None);
        };
        save_info_table(lua, &inner, &saves.current_info().header).map(Some)
    });

    // QuickSave() (`0x009EF470`, the `quick_save` shortcut when CanSave()): saves under the loc
    // text `random_localisation_strings_string_quick_save` (string id 0x133; "quick_save" in the
    // shipped loc) with the save call itself: no overwrite question, no result.
    f!("QuickSave", |_l, inner, ui, _a: Variadic<Value>| {
        let name = loc_required(&inner, "random_localisation_strings_string_quick_save");
        write_save(&inner, &ui, save_target(&inner, &name).map(|t| t.path));
        Ok(())
    });

    // QuickLoad() (`0x009EF420`, the `quick_load` shortcut): loads the newest valid save, the same
    // choice as the front end's ContinueCampaign. Single player only (the exe skips it in
    // multiplayer; ours is single player).
    f!("QuickLoad", |_l, inner, ui, _a: Variadic<Value>| {
        match latest_campaign_save(&inner.facts) {
            Some(path) => inner.requests.borrow_mut().push(UiRequest::LoadCampaign(path)),
            None => log(&inner, "QuickLoad: no save to load".into()),
        }
        Ok(())
    });

    // The multiplayer side of the root's ConfirmSave, in single player: MultiplayerPlayersName
    // (`0x009EE550`) is nil and MultiplayerPlayerOutOfTurn (`0x009EE4E0`) false when the game is not
    // multiplayer, so a single-player save shows no message. MultiplayerSaveConfirm (`0x009EE610`)
    // is reached only from the multiplayer dialogue: nothing to do here.
    f!("MultiplayerPlayersName", |_l, inner, ui, _a: Variadic<Value>| Ok(Value::Nil));
    f!("MultiplayerPlayerOutOfTurn", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("MultiplayerSaveConfirm", |_l, inner, ui, _a: Variadic<Value>| Ok(()));
    Ok(())
}
