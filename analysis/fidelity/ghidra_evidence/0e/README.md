# 0e raw Ghidra evidence — PRIVATE FORK ONLY

**Do not merge or cherry-pick this directory into the public main repo.**
The .txt files here are raw Ghidra decompilation dumps and target lists
extracted from the game executable. The project's .gitignore deliberately
excludes nalysis/**/*.txt as copyrighted game material.

They live here only so the reverse-engineering evidence backing every
\CONFIRMED\ tag in the fidelity notes is not lost with the sandbox
worktree, which is its only other copy. The owning repo is the **private**
fork \DenseCow/NapoleonRust-sandbox\; these commits are pushed to the
\sandbox\ remote and nowhere else.

When porting work to main, take the \crates/\ changes and the
\nalysis/fidelity/*.md\ reports — **cherry-pick specific commits, never
merge the branch wholesale.**
