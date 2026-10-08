# Campaign fidelity worker: run CampDecomp.java on this worker's own Ghidra copy (read-only; output stays out of git).
# Usage: run_ghidra.ps1 <targets_file> <out_file> [maxLines]
param([string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$P = "$env:USERPROFILE\Documents\NR-fc-ghidra"
$Scripts = $PSScriptRoot
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript CampDecomp.java $Targets $Out $Max *> "$Out.log"
