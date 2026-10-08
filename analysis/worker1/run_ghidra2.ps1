param([string]$Script, [string]$A1, [string]$A2)
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$W = "$env:USERPROFILE\Documents\NapoleonRust\analysis\worker1"
& "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" "$W\ghidra_project" NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath "$W\ghidra_scripts" -postScript $Script "$A1" "$A2" *> "$W\ghidra_out\last_run2.log"
