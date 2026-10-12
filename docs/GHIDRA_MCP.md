# ghidra-mcp: updating, scripts and live debugging

Read this before updating ghidra-mcp, running its scripts or using the debugger. Setup, the writer
lock and the workflow guides are in `CLAUDE.md` "ghidra-mcp".

## Updating

Branch `dev`: `exit_ghidra`, `git pull` in the install folder, then
`.venv\Scripts\python.exe -m tools.setup build` and `... deploy --ghidra-path $env:USERPROFILE\OneDrive\Desktop\RE`
(it restarts Ghidra; check `/mcp/health` shows the new build). Its settings are in that folder's
`.env`; Maven, `JAVA_HOME` and `GHIDRA_MCP_ALLOW_SCRIPTS=1` are user environment variables. Then ask
the user to reconnect ghidra-mcp in `/mcp` (the bridge is an editable install).

## Scripts and dynamic checks

The 95 scripts in `C:\ghidra-mcp-setup\ghidra-mcp\ghidra_scripts\` run with `run_ghidra_script` by
absolute path. The `.py` ones need Ghidra's Jython extension, so prefer the `.java` versions.
Dynamic checks: `emulate_function`, `analyze_dataflow`. `.luac` scripts still go through `luac_dump`.

## Live debugging

Live debugging uses Ghidra's own debugger (`debugger_launch_offers`), which needs Window > Debugger
opened once in the CodeBrowser. Launch the original with `debugger_launch`, offer
`dbgeng extra options` with `cwd` = the install folder (`dbgeng (.bat)` has no working directory, so
the game crashes on the loading screen at `0x0118757E` when a shader fails to load; plain `dbgeng`
fails), `python_executable=C:\ghidra-mcp-setup\dbg-venv\Scripts\python.exe` (a venv holding Ghidra's
bundled wheels; never pip-install them globally), and `windbg_dir=C:\Windows\System32`.

At the first loader stop, make exceptions second-chance only: `run_script_inline` finds the Debugger
tool's `DebuggerTraceManagerService` target (`getCurrent().getTarget()`, by reflection) and passes
Python to `Target.execute(src, true)`: `__import__('ghidradbg.util', fromlist=['dbg']).dbg.cmd('sxd av')`
(also `ld ud ct et eh ch dz gp ii sov`, `sxi out`); `dbg.read(addr, n)` reads live memory (Ghidra's own
memory reads can be stale). It stops twice at the loader: resume both times.

The debugger holds the game open after it quits (Task Manager can't end it): end the session by
stopping the two `python.exe ... local-dbgeng*.py` processes, then the game if it is still there.

The module is `napoleon.retail.exe`, rebased: take its base from `debugger_modules`
(dynamic = static − 0x400000 + base; `debugger_static_to_dynamic` returned the static address
unchanged on 2026-10-09). Set breakpoints only right before the action you're watching.

Logging breakpoints: run `.effmach x86` first. The evaluator is C++ (wrap MASM as
`@@masm(poi(...))`; write addresses as `0x...`, a bare `00ff50e0` sets a breakpoint at 0), with no
quotes inside the command (`bp <addr> ".catch { r eip,edi; dd @edi+0xd0 L2 }; gc"`, output to
`.logopen <file>`). Call `ghidradbg.commands.ghidra_trace_sync_disable()` first (each recorded stop
costs ~150 ms; re-enable after). Even then a site hit by every soldier each frame freezes the game:
log only sites the action alone reaches. To break in while such breakpoints run, loop
`dbg._protected_base._control.SetInterrupt(0)` until `dbg.cmd` stops raising; leftover interrupts
leave break-instruction stops to resume. The WinDbg proxy on port 8099 isn't part of this setup.

Findings go into the analysis notes as described behaviour. Ghidra holds the names and comments.
