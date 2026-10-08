# 0-C worker. Usage: run_ghidra.ps1 <exe|miles> <targets_file> <out_file> [maxLines]
# Read-only runs against the worker's own Ghidra copies; output must stay out of git (scratch folder).
#   exe   -> $env:USERPROFILE\Documents\NR-f0c-ghidra\NTW.gpr, program Napoleon.exe
#   miles -> $env:USERPROFILE\Documents\NR-miles-ghidra\MSS.gpr, program mss32.dll (imported for analysis only)
param([string]$Which, [string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "8G"
$Scripts = "$env:USERPROFILE\Documents\NR-fidelity-mw\analysis\fidelity\ghidra_scripts"
$H = "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat"
if ($Which -eq "miles") {
  & $H "$env:USERPROFILE\Documents\NR-miles-ghidra" MSS -process mss32.dll -noanalysis -readOnly -scriptPath $Scripts -postScript F0cDecomp.java $Targets $Out $Max *> "$Out.log"
} else {
  & $H "$env:USERPROFILE\Documents\NR-f0c-ghidra" NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript F0cDecomp.java $Targets $Out $Max *> "$Out.log"
}
