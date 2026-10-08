# Sandbox round 13: run CampDecomp.java on my own scratch Ghidra copy (read-only).
param([string]$Targets, [string]$Out, [string]$Max = "600")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$P = "$env:USERPROFILE\Documents\NR-0b-sandbox\target\tmp\0b_round13\ghidra_scratch"
$Scripts = "$env:USERPROFILE\Documents\NR-0b-sandbox\analysis\fidelity\campaign_ghidra"
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript CampDecomp.java $Targets $Out $Max *> "$Out.log"