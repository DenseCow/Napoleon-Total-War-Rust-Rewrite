## What changed

<!-- One or two sentences. Link the BACKLOG item or issue. -->

Closes #<!-- your claim issue's number: merging closes the claim -->

## How it was checked

<!-- Tests added or run, Ghidra addresses traced, in-game comparison with the original (with the exact command). -->

## Tags

<!-- Tags added or removed (e.g. INFERRED -> CONFIRMED), each removal with its evidence: exe address, debugger result, data file, test or in-game check. -->

## Checklist

- [ ] No files from the original game, no decompiled code, no Ghidra projects or raw Ghidra output.
- [ ] Matches the original 1:1 (or the difference is tagged `PROVISIONAL` / `PLACEHOLDER` / `INFERRED`).
- [ ] Findings written into the analysis notes for this area.
- [ ] `cargo test --workspace` passes; no new `cargo clippy` warnings; a test for each fixed bug.
- [ ] Did not run `cargo fmt`; no files rewritten by PowerShell `Set-Content` / `Out-File`.
- [ ] No personal information (names, emails, Windows usernames, personal paths).
- [ ] If an AI assistant helped: it followed `CLAUDE.md` and the ghidra-mcp workflow guides, and I reviewed its output.
