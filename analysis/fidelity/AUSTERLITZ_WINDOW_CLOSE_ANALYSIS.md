# Austerlitz Window Close Analysis

> **SUPERSEDED by `BATTLE_FIDELITY.md` §58 (4) (2026-10-05).** The root cause hypothesised below
> is **REFUTED with data**: the battle file's `duration` is **2400 s**, not 60
> (`napoleon_historical_battles\austerlitz\austerlitz_battle.xml`, measured over all 32 installed
> battle files: 1800 s … 3000 s). Nothing in the exe or the data times out at 60 s, so **Options B
> and C below must not be applied** — they would be fidelity regressions against §9's rule
> (`0x00582FB0`). The window also **does not close any more**: `--battle-key NHB_Austerlitz` with
> `--battle-ui-click wait,button_battle_start` ran a full battle to 712.1 s (`Won { side: 0 }`) with
> the window open, and `--battle --screenshot` exits 0 *with* its picture. The most likely
> explanation for the old report (INFERRED) is that a bare `--battle-key` run never leaves
> Deployment, so the exits those runs saw were harness exits. This file is kept for the audit
> trail only.

**Worker:** 0-A (Sandbox)  
**Date:** 2026-10-04  
**Branch:** `work/sandbox/0a-battle`  
**Folder:** `%USERPROFILE%\Documents\NR-sb-0a`  
**NO PUSH PERFORMED**

---

## Summary

The Austerlitz historical battle window closes approximately 60 seconds after starting. The most likely cause is the **battle file's `duration` field set to 60 seconds**, which triggers a timeout during the 64-second intro cutscene.

---

## Evidence

### 1. Austerlitz Battle Script (`target/tmp/austerlitz_battle.battle_script`)

The script contains a 64-second intro cutscene:

```lua
Cutscene_Intro = cutscene:new(bm, 
    French_Controller_French_General, 
    function() Start_Battle() end, 
    64000,  -- 64 seconds
    "Cutscene_Intro", 
    true, 
    true
)
```

After the cutscene, `Start_Battle()` is called, which registers various timers:
- `Enemy_Press_01` at 20s
- `Enemy_Press_02` at 50s  
- `Enemy_Press_03` at 310s
- `Compare_Unit_Distance_And_Enemy_Attack_01` repeating at 270s
- `Release_Control_01` repeating at 400s
- `Release_All_Control` single-shot at **600s (10 minutes)**

**No timer at exactly 60 seconds.**

### 2. Battle Timeout Logic (`crates/ntw_sim/src/battle/victory.rs`)

The victory check enforces the battle file's `duration`:

```rust
if let Some(limit) = rules.time_limit_s
    && limit >= 0.0
    && limit < battle.time_seconds()  -- battle time in seconds
{
    return rules.timeout_winner.map_or(Outcome::Draw, |side| Outcome::TimeOut { side });
}
```

- Battle time (`battle.time_seconds()`) advances only during Conflict phase (not Deployment)
- The cutscene runs during Conflict phase, so battle time advances during the 64s cutscene
- If `duration = 60`, timeout occurs at ~60s battle time (during the cutscene)

### 3. Deployment Finished Callback (`crates/napoleon/src/battle/scripts.rs`)

Correctly implemented at line 114:
```rust
BattlePhase::Conflict => {
    if s.shown == Some(BattlePhase::Deployment) || s.shown.is_none() {
        if s.shown.is_none() {
            s.host.phase("Deployment");
        }
        s.host.phase("Deployed");
        s.host.deployment_finished();  // Calls setup_battle callback
    }
}
```

The callback (`End_Deployment_Phase` → `Play_Cutscene_Intro` → cutscene) is properly invoked when phase changes to Conflict.

### 4. Ghidra Analysis (Sandbox 0-A)

Ghidra runs in `target/tmp/sb0a_f67*_out.txt` searched for:
- Formation radius writers (`+0x670`) — not found in battle code
- Garrison capacity (`+0x6C`) — type source not found
- Unit scale multiplier — reader found, writer not found

**No search for a 60-second hardcoded timer in exe.**

### 5. Window Close Behavior

From `BATTLE_FIDELITY.md` §52 (Round 17):
> "Austerlitz: both the old and the new build's windows closed about a minute into the battle ('No windows are open, exiting'), and the `--battle --screenshot` check exited 0 without saving its picture for the same reason. Not caused by this change (the old binary does it too); the Austerlitz length comparison is still to do."

- "No windows are open, exiting" = Bevy window close message
- Happens in both old and new builds
- `--battle --screenshot` exits 0 (success) without screenshot

Harness systems that exit the app:
- `auto_screenshot` (frontend): exits after 5s (4s settle + 1s)
- `fps_log` (battle): exits after `NAPOLEON_FPS_LOG` seconds (e.g., 70s)

Neither matches "about a minute" exactly unless `NAPOLEON_FPS_LOG=60`.

---

## Root Cause Hypothesis

**Tag: INFERRED**

The NHB_Austerlitz battle file (`napoleon_historical_battles/austerlitz/austerlitz_battle.xml`) likely has:
```xml
<battle_description>
    <duration>60</duration>
    <timeout_winning_alliance_index>1</timeout_winning_alliance_index>
    ...
</battle_description>
```

**Mechanism:**
1. Battle starts in Deployment phase (battle time paused)
2. Player clicks "Start Battle" (or harness clicks it) → phase becomes Conflict
3. Battle time starts advancing (0.1s per tick)
4. Script's `setup_battle` callback runs → starts 64s cutscene
5. At ~60s battle time (during cutscene), `duration < battle.time_seconds()` becomes true
6. Battle times out → `Outcome::TimeOut` → phase becomes Finished
7. HUD shows victory options → eventually window closes

**Why "old binary does it too":** The original game's battle file has the same 60s duration. The original may handle this differently (pause clock during cutscene, or longer actual duration).

---

## Verification Needed

**CONFIRMED required:** Read the actual `duration` value from the game's Austerlitz battle file.

**Path to check (in game install):**
```
data/napoleon_historical_battles/austerlitz/austerlitz_battle.xml
```
Or similar path under `napoleon_historical_battles/`.

**Command to verify (when game install available):**
```bash
# Using ntw_formats probe
cargo run -p ntw_formats --example pack_probe -- --find austerlitz_battle.xml
# Then parse and check duration field
```

---

## Potential Fixes (if CONFIRMED)

### Option A: Increase Duration (Data Fix)
If the 60s duration is a data error, update the battle file's duration to a realistic value (e.g., 3600s = 1 hour, like other historical battles).

### Option B: Pause Battle Time During Cutscenes (Code Fix)
If the original pauses the battle clock during cutscenes, implement similar logic:
- Detect when a cutscene is running (via script state or camera lock)
- Pause `battle.tick` increment during cutscenes

### Option C: Ignore Timeout During Cutscene (Code Fix)
Modify victory check to not timeout while cutscene is active:
```rust
// In victory::check or Battle::step
if battle.is_cutscene_active() { return Outcome::Ongoing; }
```

---

## Files Referenced

| File | Purpose |
|------|---------|
| `target/tmp/austerlitz_battle.battle_script` | Austerlitz Lua script (64s cutscene) |
| `crates/ntw_sim/src/battle/victory.rs` | Timeout logic |
| `crates/napoleon/src/battle/scripts.rs` | Phase handling, deployment_finished callback |
| `crates/ntw_formats/src/battle_spec.rs` | Battle file parsing (duration field) |
| `analysis/fidelity/BATTLE_FIDELITY.md` | Fidelity documentation |
| `target/tmp/sb0a_f67*_out.txt` | Ghidra analysis outputs |

---

## Conclusion

**Lead:** The battle file's `duration=60` is the most probable cause. The 64s cutscene exceeds the 60s limit, triggering a timeout.

**Next Step:** Verify the `duration` value in the actual Austerlitz battle file from the game install. If confirmed, decide on fix strategy (data vs code).

**NO PUSH PERFORMED** — Analysis only.