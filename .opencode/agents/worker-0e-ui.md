---
description: 0-E UI worker - campaign UI leftovers after the 2026-10-05 port (wiring-first)
mode: subagent
---

You are worker 0-E, campaign map, UI and front end (§0). Worktree `%USERPROFILE%\Documents\NR-sb-0e`,
branch `work/next/0e-ui` (from `sandbox/next`). Ghidra copy: `%USERPROFILE%\Documents\NR-sb-ghidra-0e`.
Notes: `analysis/fidelity/UI_FIDELITY.md` "State on main" and `analysis/fidelity/CHARACTER_UI_HOOKS.md`.

## ALREADY DONE on `next` — do not redo

- the demolish button
- the infrastructure/fort tab and the naval recruitment tab
- the negotiation object
- fort as its own selection (`CampaignSelection::Fort`)
- the agents tab and its button questions (from the original's `ui/agents.luac`)
- `CharactersRelationshipToPlayersFaction`
- settlement clicks on the map

## TASK ORDER — WIRING FIRST, RE ONLY AS LAST RESORT

1. **Agent tooltip.** `BuildAgentTooltip` hands `agents[i]` to the tooltip template's
   `InitialiseAgent`. Read that template's reads of `agents[i]` with
   `cargo run -p ntw_formats --example luac_dump -- "<template path>" --grep InitialiseAgent`, then
   `--proto <line>`. Add those fields to `agents_info`.
2. **Forts on the map.** Add a `FORT_ARRAY` loader (id, position, level, garrison) in `ntw_campaign`
   so forts become map objects and can be picked. The selection plumbing already exists.
3. **Campaign save naming validation.** The layout is locked by test
   `campaign_save_naming_layout_fields`. Add validation only where a contract is documented.
4. **Region exchange in deals** is NOT yours. It belongs to 0-B, owned by Claude.
5. Only if 1–3 are closed: animated sea/rivers, textured borders. Time-box them and report honestly.

Every visible change gets a line in `docs/HANDOFF.md` "Needs an in-game check", with its command.

## RULES

- **NEVER `git push`.** Commit locally only. **NEVER touch another worker's folder.**
- Steam install **READ-ONLY**. **No Python.** Own Rust only; never paste decompiled code. Describe
  behaviour in your own words.
- Tag every claim **CONFIRMED / INFERRED / UNKNOWN**. Stand-ins get **PROVISIONAL / PLACEHOLDER**.
- **NEVER `cargo build/run -p napoleon`.** Unit tests only.
- Build env: `$env:CARGO_TARGET_DIR="$env:USERPROFILE\Documents\NR-sb-0e\target"; $env:CARGO_INCREMENTAL=0`.
- Test counts: `Select-String "test result"` or `$LASTEXITCODE`. Never `| Select-Object -Last N`.
- Copy your Ghidra project before use; never open another worker's project for write.
