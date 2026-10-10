//! Loads the game database once, when the program starts.
//!
//! If the original game is installed, the real tables are read from its `.pack` files
//! (read-only). If not, we fall back to `ntw_data`'s test fixture: clearly made-up
//! numbers, so the program still runs on a machine without the game.

use bevy::prelude::*;
use ntw_data::GameDatabase;

use crate::config;

/// A Bevy *resource* (a single global value that systems can read) holding the database.
#[derive(Resource)]
pub struct GameData {
    /// All loaded tables.
    pub db: GameDatabase,
    /// One line describing where the data came from (the debug HUD showed it; kept for logs).
    #[allow(dead_code)]
    pub source_text: String,
}

/// Registers the `GameData` resource.
pub struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        // We load in `build` (before any system runs), so every later system can rely on it.
        app.insert_resource(load());
    }
}

fn load() -> GameData {
    let dir = config::game_data_dir();
    match GameDatabase::from_install(&dir) {
        Ok(db) => {
            info!("Loaded game data from {}", dir.display());
            for w in &db.load_warnings {
                warn!("Game data: {w}");
            }
            GameData {
                db,
                source_text: "Data: your Napoleon: Total War install (read-only)".to_string(),
            }
        }
        Err(e) => {
            warn!("Could not load game data from {}: {e}. Using the test fixture.", dir.display());
            GameData {
                db: GameDatabase::test_fixture(),
                source_text: format!(
                    "Data: TEST FIXTURE (made-up numbers). Game not found at {}",
                    dir.display()
                ),
            }
        }
    }
}
