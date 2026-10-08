# Test the usage budget (tools/usage_budget.ps1).
#   selftest : runs every rule against fake readings in a scratch folder. No Claude needed.
#   warn     : fake usage for a live test: 5-hour window at 92%, resetting in 5 minutes.
#   stop     : fake usage: 5-hour window at 97% (night mode stops).
#   refresh  : fake usage after the refresh: everything low again.
#   off      : back to the real readings.
# Blocks: the selftest opens a block at week 16% (cap 28%). The status line shows SIMULATED while it is on.
param([string]$Cmd = 'help')

$budget  = Join-Path $PSScriptRoot 'usage_budget.ps1'
$realDir = Join-Path $env:LOCALAPPDATA 'napoleon-usage'
$simFile = Join-Path $realDir 'sim.json'

function Now-Sec { [DateTimeOffset]::Now.ToUnixTimeSeconds() }
# resets_at for a weekly window in which today is day $day.
function Resets-For([int]$day) {
    # The weekly window started ($day - 1) days and one hour ago, so now is one hour into day $day.
    (Now-Sec) - ($day - 1) * 86400 - 3600 + 7 * 86400
}

function Write-Sim([double]$week, [double]$h5, [long]$h5ResetsIn) {
    if (-not (Test-Path $realDir)) { New-Item -ItemType Directory -Path $realDir -Force | Out-Null }
    [pscustomobject]@{ week = $week; resets_at = (Resets-For 2); h5 = $h5; h5_resets = (Now-Sec) + $h5ResetsIn } |
        ConvertTo-Json -Compress | Set-Content -Path $simFile -Encoding ASCII
    Remove-Item (Join-Path $realDir 'warned.txt') -ErrorAction SilentlyContinue
    Write-Host "Simulated: week $week%, 5-hour window $h5%. Applies at Claude's next reply."
}

switch ($Cmd) {
    'warn'    { Write-Sim 10 92 300 }
    'stop'    { Write-Sim 10 97 300 }
    'refresh' { Write-Sim 10 5 18000 }
    'off'     { Remove-Item $simFile -ErrorAction SilentlyContinue; Write-Host 'Simulation off: real readings from the next reply.' }
    'selftest' {
        $failed = 0
        $env:LOCALAPPDATA = Join-Path ([IO.Path]::GetTempPath()) ('usage-selftest-' + (Now-Sec))
        $dir = Join-Path $env:LOCALAPPDATA 'napoleon-usage'
        function Run([string]$mode, [string]$stdin) {
            Remove-Item (Join-Path $dir 'warned.txt') -ErrorAction SilentlyContinue
            $a = @($mode -split ' ')
            ($stdin | powershell -NoProfile -ExecutionPolicy Bypass -File $budget @a) -join ''
        }
        function Feed([double]$week, [int]$day, [double]$h5) {
            $rl = @{ seven_day = @{ used_percentage = $week; resets_at = (Resets-For $day) };
                     five_hour = @{ used_percentage = $h5; resets_at = (Now-Sec) + 7200 } }
            $null = Run 'status' (@{ rate_limits = $rl } | ConvertTo-Json -Compress -Depth 4)
        }
        function Expect([string]$name, [string]$got, [string]$want) {
            $ok = switch ($want) {
                'none'   { $got -eq '' }
                'wrap'   { $got -like '*additionalContext*' -and $got -notlike '*CronCreate*' }
                'resume' { $got -like '*additionalContext*' -and $got -like '*CronCreate*' }
                'stop'   { $got -like '*"continue":false*' }
                default  { $got -like "*$want*" }
            }
            if ($ok) { Write-Host "  ok    $name" } else { Write-Host "  FAIL  $name -> $got"; $script:failed++ }
        }
        Write-Host 'Day mode (block opened at 16%, cap 28%):'
        $env:NAPOLEON_USAGE_MODE = 'day'
        Feed 16 2 40;   Expect 'a block opens at 16%'         (Get-Content (Join-Path $dir 'block.json') -Raw) '"week":16'
        Feed 20 2 40;   Expect 'week 20%: keeps working'      (Run 'post' '{}') 'none'
        Feed 26.1 2 40; Expect 'week 26.1%: wrap-up warning'  (Run 'post' '{}') 'wrap'
        Feed 28 2 40;   Expect 'week 28%: stops'              (Run 'post' '{}') 'stop'
        Feed 10 2 99;   Expect '5h 99%: ignored in day mode'  (Run 'post' '{}') 'none'
        Write-Host 'Night mode (cap 28%, stop at 26.5%):'
        $env:NAPOLEON_USAGE_MODE = 'night'
        Expect 'session start note'                           (Run 'start' '{}') 'NIGHT MODE'
        Feed 24.6 2 40; Expect 'week 24.6%: wrap-up, no resume' (Run 'post' '{}') 'wrap'
        Feed 26.5 2 40; Expect 'week 26.5%: stops'            (Run 'post' '{}') 'stop'
        Expect 'budget check is never stopped' (Run 'post' '{"tool_input":{"command":"tools/usage_budget.ps1 check"}}') 'none'
        Expect 'check at the block cap: STOP'                 (Run 'check' '') 'STOP: this block'
        @{ week = 16; time = (Now-Sec) - 72000 } | ConvertTo-Json -Compress | Set-Content (Join-Path $dir "block.json")
        Feed 26.5 2 40; Expect "20 h later a new block: ALLOWED" (Run 'check' '') 'ALLOWED'
        @{ week = 16; time = (Now-Sec) } | ConvertTo-Json -Compress | Set-Content (Join-Path $dir "block.json")
        Feed 10 2 92;   Expect '5h 92%: wrap-up + resume'     (Run 'post' '{}') 'resume'
        Feed 10 2 97;   Expect '5h 97%: stops'                (Run 'post' '{}') 'stop'
        Write-Host 'Switching mode in chat (launched in day mode):'
        $env:NAPOLEON_USAGE_MODE = 'day'
        Expect 'mode night prints the night rules'            (Run 'mode night' '') 'NIGHT MODE'
        Feed 26.5 2 40; Expect 'then night rules apply (stop at 26.5%)' (Run 'post' '{}') 'stop'
        Expect 'mode day switches back'                       (Run 'mode day' '') 'day mode'
        Feed 26.5 2 40; Expect 'then day rules apply (26.5% is a warning)' (Run 'post' '{}') 'wrap'
        Remove-Item $env:LOCALAPPDATA -Recurse -Force -ErrorAction SilentlyContinue
        if ($failed -eq 0) { Write-Host 'All checks passed.' } else { Write-Host "$failed check(s) FAILED." }
    }
    default { Get-Content $PSCommandPath -TotalCount 7 | ForEach-Object { $_.TrimStart('# ') } }
}
