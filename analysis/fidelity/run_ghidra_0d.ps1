# 0-D worker (units / animation / terrain / trees). Read-only Ghidra runs.
# Usage: run_ghidra_0d.ps1 <script> <targets> <out> [maxLines]
#   The project copy is this worker's own: $env:USERPROFILE\Documents\NR-sb-0d-ghidra (a copy of
#   the retired 0-A project). Never open another worker's project for write.
#   Example:
#     .\analysis\fidelity\run_ghidra_0d.ps1 UnitDecomp.java "0x006631A0" target/tmp/0d_g1.txt 900
param([string]$Script, [string]$Targets, [string]$Out, [string]$Max = "900")
$env:GHIDRA_HEADLESS_MAXMEM = "8G"
$Scripts = "$env:USERPROFILE\Documents\NR-sb-0d\analysis\fidelity\ghidra_scripts"
$H = "$env:USERPROFILE\OneDrive\Desktop\GTA SA - 2\support\analyzeHeadless.bat"
$P = "$env:USERPROFILE\Documents\NR-sb-0d-ghidra"
& $H $P NTW -process Napoleon.exe -noanalysis -readOnly -scriptPath $Scripts -postScript $Script $Targets $Out $Max *> "$Out.log"
Write-Output "log $Out.log"