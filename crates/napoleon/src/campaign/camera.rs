//! The campaign camera: a target point on the map, a distance and a tilt that follows the zoom.
//!
//! CONFIRMED (tweaker defaults in `Napoleon.exe`, `CampaignCamera.cpp` lines 91-100 and
//! `EmpireCampaignController.cpp` 70-71, read with `analysis/worker1/re_tools tweak`):
//! minimum distance 15, maximum distance 100, tilt from straight down (-y) between 0.5 and 1.0
//! radians, field of view 57.2957 (= 1 radian), pan speed 50, tilt speed 50.
//!
//! INFERRED / PROVISIONAL (our own until the CampaignCamera code is read in Ghidra):
//! - the tilt is 1.0 rad when fully zoomed in and 0.5 rad when fully zoomed out, linear in
//!   the distance (the Lua help says zoom is "clamped at 1.2 - 0.675, see ..._TILT_ANGLE");
//! - the field of view is vertical;
//! - the pan speed scales with the distance (`distance` units per second), and "50" is not used;
//! - the target is clamped to the theatre bounds from `regions.esf`;
//! - no camera rotation (the original campaign camera always faces north, as far as we know).

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

/// Tweaker `CAMPAIGN_CAMERA_MINIMUM_DISTANCE` (CONFIRMED default).
pub const MIN_DISTANCE: f32 = 15.0;
/// Tweaker `CAMPAIGN_CAMERA_MAXIMUM_DISTANCE` (CONFIRMED default).
pub const MAX_DISTANCE: f32 = 100.0;
/// Tweaker `CAMPAIGN_CAMERA_MINIMUM_TILT_ANGLE`, radians from -y (CONFIRMED default).
pub const MIN_TILT: f32 = 0.5;
/// Tweaker `CAMPAIGN_CAMERA_MAXIMUM_TILT_ANGLE`, radians from -y (CONFIRMED default).
pub const MAX_TILT: f32 = 1.0;
/// Tweaker field of view, 57.2957 degrees (CONFIRMED default).
pub const FOV_DEGREES: f32 = 57.2957;

/// The camera's state, in Bevy coordinates (x east, z south).
#[derive(Component, Debug, Clone, Copy)]
pub struct CampaignCamera {
    /// The point looked at (Bevy x, z).
    pub target: Vec2,
    /// Distance from the target.
    pub distance: f32,
    /// Allowed target area (Bevy x/z min and max).
    pub min: Vec2,
    /// Allowed target area max.
    pub max: Vec2,
    /// Ground height at the target (updated by the scene).
    pub ground: f32,
}

impl CampaignCamera {
    /// Tilt from straight down for the current distance (INFERRED linear law).
    pub fn tilt(&self) -> f32 {
        let t = ((self.distance - MIN_DISTANCE) / (MAX_DISTANCE - MIN_DISTANCE)).clamp(0.0, 1.0);
        MAX_TILT + (MIN_TILT - MAX_TILT) * t
    }

    /// The camera transform for this state.
    pub fn transform(&self) -> Transform {
        let tilt = self.tilt();
        let focus = Vec3::new(self.target.x, self.ground, self.target.y);
        // Looking north (-Z): the camera sits south (+Z) of the target and above it.
        let eye = focus + self.distance * Vec3::new(0.0, tilt.cos(), tilt.sin());
        Transform::from_translation(eye).looking_at(focus, Vec3::Y)
    }
}

/// WASD / arrow keys pan, the mouse wheel (or PageUp/PageDown) zooms, dragging with the
/// right or middle mouse button pans.
pub fn control(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time<Real>>,
    ground: Option<Res<super::scene::CampaignGround>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    mut cams: Query<(&mut CampaignCamera, &mut Transform)>,
) {
    let dt = time.delta_secs();
    // Harness runs (`--screenshot`) ignore the live keyboard and mouse so captures are repeatable.
    let live = harness.is_none();
    for (mut cam, mut tf) in &mut cams {
        if !live {
            if let Some(g) = &ground {
                cam.ground = g.0.height_at(cam.target.x, -cam.target.y);
            }
            *tf = cam.transform();
            continue;
        }
        let mut pan = Vec2::ZERO;
        let pressed = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
        if pressed(KeyCode::KeyW, KeyCode::ArrowUp) {
            pan.y -= 1.0;
        }
        if pressed(KeyCode::KeyS, KeyCode::ArrowDown) {
            pan.y += 1.0;
        }
        if pressed(KeyCode::KeyA, KeyCode::ArrowLeft) {
            pan.x -= 1.0;
        }
        if pressed(KeyCode::KeyD, KeyCode::ArrowRight) {
            pan.x += 1.0;
        }
        let speed = cam.distance;
        cam.target += pan * speed * dt;
        if mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) {
            let d = cam.distance;
            cam.target -= motion.delta * d * 0.002;
        }
        let mut zoom = -scroll.delta.y;
        if keys.pressed(KeyCode::PageUp) {
            zoom -= 5.0 * dt;
        }
        if keys.pressed(KeyCode::PageDown) {
            zoom += 5.0 * dt;
        }
        if zoom != 0.0 {
            cam.distance = (cam.distance * 1.12f32.powf(zoom)).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
        let (min, max) = (cam.min, cam.max);
        cam.target = cam.target.clamp(min, max);
        if let Some(g) = &ground {
            cam.ground = g.0.height_at(cam.target.x, -cam.target.y);
        }
        *tf = cam.transform();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilt_follows_zoom() {
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: MIN_DISTANCE, min: Vec2::splat(-1.0), max: Vec2::splat(1.0), ground: 0.0 };
        assert!((c.tilt() - MAX_TILT).abs() < 1e-6);
        c.distance = MAX_DISTANCE;
        assert!((c.tilt() - MIN_TILT).abs() < 1e-6);
        let t = c.transform();
        // Zoomed out: above and south of the target, looking north and down.
        assert!(t.translation.y > 0.0 && t.translation.z > 0.0);
        assert!(t.forward().z < 0.0 && t.forward().y < 0.0);
    }
}
