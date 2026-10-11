//! Shared "screenshot, quit once saved" harness used by `--screenshot` (frontend) and
//! `NAPOLEON_AI_SHOT` (battle_ai). It saves the picture itself instead of using Bevy's
//! `save_to_disk`, because that one logs a failed write but still fires `ScreenshotCaptured`,
//! so a harness run would look like a success.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Real seconds to wait for the GPU read-back and the file write before giving up.
const TIMEOUT_SECS: f32 = 15.0;

// Pending is 0 (the default).
const SAVED: u8 = 1;
const FAILED: u8 = 2;
const TIMED_OUT: u8 = 3;

/// Result of one requested screenshot, shared with the observer that writes the file.
#[derive(Clone, Default)]
pub struct ShotOutcome(Arc<AtomicU8>);

impl ShotOutcome {
    /// Asks for a picture of the primary window; the file is written when the read-back arrives.
    pub fn request(&self, commands: &mut Commands, path: PathBuf) {
        let state = self.0.clone();
        commands.spawn(Screenshot::primary_window()).observe(move |captured: On<ScreenshotCaptured>| {
            let result = write_rgb_image(&captured.image, &path);
            match &result {
                Ok(()) => info!("Screenshot saved to {}", path.display()),
                Err(e) => error!("Cannot save screenshot to {}: {e}", path.display()),
            }
            state.store(if result.is_ok() { SAVED } else { FAILED }, Ordering::Relaxed);
        });
    }

    /// The exit to send now, if the run is over: success once the file is on disk, an error if
    /// the write failed or `waited` real seconds since the request passed the timeout. A failed
    /// write was already logged by the observer; the timeout is logged here (`what` names the
    /// harness).
    pub fn exit(&self, waited: f32, what: &str, path: &Path) -> Option<AppExit> {
        match self.0.load(Ordering::Relaxed) {
            SAVED => Some(AppExit::Success),
            FAILED => Some(AppExit::error()),
            TIMED_OUT => Some(AppExit::error()),
            _ if waited > TIMEOUT_SECS => {
                self.0.store(TIMED_OUT, Ordering::Relaxed);
                warn!("{what}: no picture was saved to {} within {TIMEOUT_SECS} s of the capture request", path.display());
                Some(AppExit::error())
            }
            _ => None,
        }
    }
}

fn write_rgb_image(image: &Image, path: &Path) -> Result<(), String> {
    let dynamic = image.clone().try_into_dynamic().map_err(|e| format!("convert: {e}"))?;
    // Drop the alpha channel (it holds brightness values when HDR is on), like Bevy's own saver.
    dynamic.to_rgb8().save(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    fn tiny() -> Image {
        Image::new_fill(
            Extent3d { width: 2, height: 2, depth_or_array_layers: 1 },
            TextureDimension::D2,
            &[10, 20, 30, 255],
            TextureFormat::Rgba8UnormSrgb,
            default(),
        )
    }

    #[test]
    fn writes_a_picture() {
        let path = std::env::temp_dir().join(format!("nr_shot_ok_{}.png", std::process::id()));
        write_rgb_image(&tiny(), &path).unwrap();
        assert!(path.metadata().unwrap().len() > 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn failed_write_is_an_error() {
        let path = std::env::temp_dir().join("nr_no_such_dir_polish10").join("x.png");
        assert!(write_rgb_image(&tiny(), &path).is_err());
    }

    #[test]
    fn exit_follows_outcome() {
        let p = Path::new("x.png");
        let o = ShotOutcome::default();
        assert!(o.exit(1.0, "t", p).is_none());
        assert!(matches!(o.exit(16.0, "t", p), Some(AppExit::Error(_))));
        assert!(matches!(o.exit(0.0, "t", p), Some(AppExit::Error(_))), "stays an error, without logging again");
        o.0.store(SAVED, Ordering::Relaxed);
        assert!(matches!(o.exit(1.0, "t", p), Some(AppExit::Success)));
        o.0.store(FAILED, Ordering::Relaxed);
        assert!(matches!(o.exit(1.0, "t", p), Some(AppExit::Error(_))));
    }
}
