# Starts Claude Code on this repo after asking for the usage mode.
# Run start-claude.bat in the repo root, or: powershell -ExecutionPolicy Bypass -File tools\start-claude.ps1
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

Write-Host ''
Write-Host 'NapoleonRust - choose a usage mode'
Write-Host ''
Write-Host '  [D] Day mode   - you are around.'
Write-Host '                   Each block may use 11 points of the weekly limit, from the reading when it opened.'
Write-Host '                   A new block opens 20 hours or more after the last one, so one block a day.'
Write-Host '                   Claude is told to wrap up 2 points before the cap and stops at the cap.'
Write-Host '                   It may pass the cap slightly while finishing its last step.'
Write-Host ''
Write-Host '  [N] Night mode - you are asleep. A strict cap that is never passed.'
Write-Host '                   Same blocks, but Claude is stopped 1.5 points below the cap'
Write-Host '                   (told to wrap up 2 points before that), or at 97% of the 5-hour window.'
Write-Host '                   Before stopping, Claude schedules a resume for the next refresh (the'
Write-Host '                   5-hour reset), so keep this window open and it carries on.'
Write-Host ''

$mode = $null
while ($null -eq $mode) {
    $answer = (Read-Host 'Day or Night? [D/N]').Trim().ToUpper()
    if ($answer -eq 'D') { $mode = 'day' }
    elseif ($answer -eq 'N') { $mode = 'night' }
}
$env:NAPOLEON_USAGE_MODE = $mode
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'usage_budget.ps1') mode $mode | Out-Null
Write-Host "Starting Claude in $mode mode."

git pull
claude --permission-mode acceptEdits
