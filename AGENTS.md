# Instructions for AI coding agents

This project's rules live in one file: **[`CLAUDE.md`](CLAUDE.md)**. Read it before doing anything,
starting with its "Outside contributors" section, and follow it whichever assistant you are.
[`CONTRIBUTING.md`](CONTRIBUTING.md) covers the setup, including
[ghidra-mcp](https://github.com/bethington/ghidra-mcp) and its workflow guides in `docs/prompts/`.

The rules that matter most:

- Never commit files from the original game, decompiled code, Ghidra projects or raw Ghidra output.
- Match the original 1:1. Trace exe behaviour in Ghidra before relying on it; tag anything not
  verified `INFERRED`, `PROVISIONAL` or `PLACEHOLDER`, and remove a tag only with cited evidence.
  Without ghidra-mcp, take only work that doesn't depend on the exe's behaviour; never guess it.
- This is a modding platform for any Total War game, with Napoleon 1:1 as its first game: keep engine code
  generic and data-driven (no Napoleon-only keys or assumptions in code). A gameplay bug in the original,
  traced in Ghidra, is fixed, with no toggle to restore it (`ORIGINAL BUG:` in the code).
- Keep `cargo test --workspace` passing and add no `cargo clippy` warnings. Never run `cargo fmt`.
- No Python, no personal information, and never push to `main`: open a pull request from a fork.
