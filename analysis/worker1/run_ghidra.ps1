param([string]$Targets, [string]$Out, [int]$MaxLines = 260, [string]$Script = "DecompTargets.java")
$env:GHIDRA_HEADLESS_MAXMEM = "12G"
$W = "$env:USERPROFILE\Documents\NapoleonRust\analysis\worker1"
if ($Script -eq "DecompTargets.java") {
  & "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" "$W\ghidra_project" NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath "$W\ghidra_scripts" -postScript DecompTargets.java "$Targets" "$Out" $MaxLines *> "$W\ghidra_out\last_run.log"
} else {
  & "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat" "$W\ghidra_project" NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath "$W\ghidra_scripts" -postScript $Script "$Out" *> "$W\ghidra_out\last_run.log"
}
