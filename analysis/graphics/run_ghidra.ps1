# Shaders worker: run GfxDecomp on the main Ghidra project (read-only). Usage: run_ghidra.ps1 <targets.txt> <out.c> [maxLines]
param([string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$P = "$env:USERPROFILE\Documents\NapoleonRust\analysis\worker1\ghidra_project"
$S = "$env:USERPROFILE\Documents\NR-shaders\analysis\graphics\ghidra_scripts"
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $S -postScript GfxDecomp.java "$Targets" "$Out" $Max *> "$env:USERPROFILE\Documents\NR-shaders\analysis\graphics\ghidra_out\last_run.log"
