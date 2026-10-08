# s1-leftovers worker. Usage: run_ghidra_s1.ps1 <targets_file> <out_file> [maxLines]
# Read-only runs against the worker's own Ghidra copy; output must stay out of git (scratch folder).
#   $env:USERPROFILE\Documents\NR-s1-ghidra\NTW.gpr, program Napoleon.exe
param([string]$Targets, [string]$Out, [string]$Max = "400")
$env:GHIDRA_HEADLESS_MAXMEM = "8G"
$Scripts = "$env:USERPROFILE\Documents\NR-s1-leftovers\analysis\campaign\ghidra_scripts"
$H = "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat"
& $H "$env:USERPROFILE\Documents\NR-s1-ghidra" NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript S1Decomp.java $Targets $Out $Max *> "$Out.log"
