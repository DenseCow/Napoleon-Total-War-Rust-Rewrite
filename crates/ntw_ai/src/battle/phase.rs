//! The battlegroup's **encounter phase** (`analysis/ai/AI_RESEARCH.md` §3.1b), CONFIRMED.
//!
//! The battlegroup keeps its phase at `+0x48` (display `0x00765370`). Each planner step computes
//! it (`0x0076C0B0`): the engagement phase (`0x0076C300`) if that is not NONE, else the distance
//! phase (`0x0076D250`) if that is not NONE, else MOVING. The tactic scores and keep tests read it
//! (STOP_AND_SHOOT needs CONTACT and stops at GENERAL MELEE; OUTFLANK wants CLOSE APPROACH or
//! CONTACT; DOUBLE_ENVELOPMENT CONTACT).

/// Encounter phases (`0x00765370`, CONFIRMED names and values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Phase {
    /// 0.
    #[default]
    None = 0,
    /// 1.
    Waiting = 1,
    /// 2.
    Moving = 2,
    /// 3.
    DistantApproach = 3,
    /// 4.
    CloseApproach = 4,
    /// 5.
    Contact = 5,
    /// 6.
    InitialAssault = 6,
    /// 7.
    GeneralMelee = 7,
}

/// `0x0076C300`: from the share of units engaged in melee (`0x0054EE90` or `0x00797850`) and the
/// share of the others that are shooting (`0x0057A150(1)` and `0x0055AFE0`), in integer percent of
/// the group's `n` units: GENERAL MELEE when engaged ≥ 80, or engaged ≥ 40 and engaged + shooting
/// ≥ 80; INITIAL ASSAULT when engaged ≥ 30; CONTACT when engaged + shooting ≥ 50; else NONE.
/// Hysteresis: from GENERAL MELEE, CONTACT and INITIAL ASSAULT stay GENERAL MELEE; from INITIAL
/// ASSAULT, CONTACT stays INITIAL ASSAULT. An empty group gives INITIAL ASSAULT.
pub fn engagement_phase(n: u32, engaged: u32, shooting: u32, current: Phase) -> Phase {
    if n == 0 {
        return Phase::InitialAssault;
    }
    let e = engaged * 100 / n;
    let s = shooting * 100 / n + e;
    let p = if e >= 80 || (e >= 40 && s >= 80) {
        Phase::GeneralMelee
    } else if e >= 30 {
        Phase::InitialAssault
    } else if s >= 50 {
        Phase::Contact
    } else {
        Phase::None
    };
    match (current, p) {
        (Phase::GeneralMelee, Phase::Contact | Phase::InitialAssault) => Phase::GeneralMelee,
        (Phase::InitialAssault, Phase::Contact) => Phase::InitialAssault,
        _ => p,
    }
}

/// `0x0076D250`: from the squared distance to the target (the battlegroup's virtual `+0x0C`):
/// CONTACT within 175 m (30625 m², kept up to 61250 m² once in CONTACT), CLOSE APPROACH within 300 m
/// (90000, kept up to 180000), DISTANT APPROACH within 750 m (562500, kept up to 1125000), else
/// NONE; NONE with no target.
pub fn distance_phase(d2: Option<f32>, current: Phase) -> Phase {
    let Some(d2) = d2 else { return Phase::None };
    if d2 <= 30625.0 || (current == Phase::Contact && d2 < 61250.0) {
        Phase::Contact
    } else if d2 <= 90000.0 || (current == Phase::CloseApproach && d2 < 180000.0) {
        Phase::CloseApproach
    } else if d2 <= 562500.0 || (current == Phase::DistantApproach && d2 < 1_125_000.0) {
        Phase::DistantApproach
    } else {
        Phase::None
    }
}

/// `0x0076C0B0`: the engagement phase, else the distance phase, else MOVING.
pub fn phase(n: u32, engaged: u32, shooting: u32, d2: Option<f32>, current: Phase) -> Phase {
    match engagement_phase(n, engaged, shooting, current) {
        Phase::None => match distance_phase(d2, current) {
            Phase::None => Phase::Moving,
            p => p,
        },
        p => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engagement_thresholds() {
        assert_eq!(engagement_phase(10, 8, 0, Phase::None), Phase::GeneralMelee);
        assert_eq!(engagement_phase(10, 4, 4, Phase::None), Phase::GeneralMelee);
        assert_eq!(engagement_phase(10, 3, 0, Phase::None), Phase::InitialAssault);
        assert_eq!(engagement_phase(10, 1, 4, Phase::None), Phase::Contact);
        assert_eq!(engagement_phase(10, 0, 4, Phase::None), Phase::None);
        // Hysteresis.
        assert_eq!(engagement_phase(10, 3, 0, Phase::GeneralMelee), Phase::GeneralMelee);
        assert_eq!(engagement_phase(10, 1, 4, Phase::InitialAssault), Phase::InitialAssault);
        assert_eq!(engagement_phase(0, 0, 0, Phase::None), Phase::InitialAssault);
    }

    #[test]
    fn distance_thresholds_with_hysteresis() {
        assert_eq!(distance_phase(Some(170.0 * 170.0), Phase::None), Phase::Contact);
        assert_eq!(distance_phase(Some(200.0 * 200.0), Phase::None), Phase::CloseApproach);
        assert_eq!(distance_phase(Some(200.0 * 200.0), Phase::Contact), Phase::Contact);
        assert_eq!(distance_phase(Some(400.0 * 400.0), Phase::CloseApproach), Phase::CloseApproach);
        assert_eq!(distance_phase(Some(400.0 * 400.0), Phase::None), Phase::DistantApproach);
        assert_eq!(distance_phase(Some(900.0 * 900.0), Phase::None), Phase::None);
        assert_eq!(phase(5, 0, 0, Some(900.0 * 900.0), Phase::None), Phase::Moving);
        assert_eq!(phase(5, 0, 0, None, Phase::None), Phase::Moving);
    }
}
