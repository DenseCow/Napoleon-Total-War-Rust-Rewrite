# Usage: run_ghidra.ps1 <targets_file> <out_file> [maxLines]   (read-only; output must stay out of git)
param([string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$P = "$env:USERPROFILE\Documents\NapoleonRust\analysis\worker1\ghidra_project"
$Scripts = "$env:USERPROFILE\Documents\NR-audio\analysis\audio\ghidra_scripts"
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript AudioDecomp.java $Targets $Out $Max *> "$Out.log"
