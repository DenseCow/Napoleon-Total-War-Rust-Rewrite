# Save-compat worker: run SaveDecomp.java on the NR-spt-ghidra copy (read-only; output must stay out of git).
# Usage: run_ghidra.ps1 <targets_file> <out_file> [maxLines]
param([string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$P = "$env:USERPROFILE\Documents\NR-spt-ghidra"
$Scripts = Join-Path $PSScriptRoot "ghidra_scripts"
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript SaveDecomp.java $Targets $Out $Max *> "$Out.log"
