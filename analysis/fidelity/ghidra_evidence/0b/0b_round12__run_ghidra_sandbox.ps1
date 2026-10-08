# Sandbox version: run CampDecomp.java on the sandbox Ghidra copy (read-only).
param([string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$P = "$env:USERPROFILE\Documents\NR-0b-sandbox\NR-fc-ghidra"
$Scripts = "$env:USERPROFILE\Documents\NR-0b-sandbox\analysis\fidelity\campaign_ghidra"
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript CampDecomp.java $Targets $Out $Max *> "$Out.log"
