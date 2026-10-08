# Usage blocks for Claude Code sessions (Claude Max weekly limit).
# Each work block may use $BlockPercent points of the weekly limit, counted from the weekly reading
# when the block opened. A new block opens at the first reading $BlockGapSec or more after the last
# one opened, so one block a day at about the same time (user plan, 2026-10-07: about 12 points a day
# from Wed 2026-10-07 to the reset on Wed 2026-10-14). The block survives restarts and compaction.
# Mode comes from $env:NAPOLEON_USAGE_MODE, set by tools/start-claude.ps1 (default: day).
#   day   : wrap up 2 points before the block cap, stop at the cap; missing/stale data lets work continue.
#   night : strict. Stop 1.5 points below the cap (wrap up 2 points before that), stop at 97% of
#           the 5-hour window, and stop if the usage reading is more than 30 minutes old.
#           Resume rule (user, 2026-10-07): when the 5-hour window stops a night session while
#           the block still has room, Claude schedules a resume at the window's refresh
#           (CronCreate) and keeps going until the block cap. A stop on the block cap itself
#           is final: no resume until the user starts the next session.
# Commands:
#   status : status line. Saves rate_limits from stdin, prints the block.
#   start  : SessionStart hook. In night mode, tells Claude the rules.
#   post   : PostToolUse hook. Wrap-up notice near the stop point, then stop.
#   check  : run by Claude when a scheduled resume fires. Prints ALLOWED, or WAIT and the next cron.
#   mode day|night : switch mode mid-session (saved in mode.txt, which overrides the launch choice).
param([string]$Mode, [string]$Arg)

$BlockPercent = 11.0     # weekly-limit points one block may use (12 planned; 11 because the reading lags the usage page by ~1 point, user 2026-10-07)
$BlockGapSec  = 72000    # 20 hours: the earliest a new block opens after the last one opened
$WrapAt       = 2.0      # warn this many points before the stop point
$H5Wrap       = 90.0     # night: 5-hour window percentage that starts the wrap-up
$H5Stop       = 97.0     # night: 5-hour window percentage that stops the session
$WarnEverySec = 600
$StaleSec     = 1800

$dir       = Join-Path $env:LOCALAPPDATA 'napoleon-usage'
$rateFile  = Join-Path $dir 'rate.json'
$warnFile  = Join-Path $dir 'warned.txt'
$modeFile  = Join-Path $dir 'mode.txt'
$blockFile = Join-Path $dir 'block.json'

$modeName = $env:NAPOLEON_USAGE_MODE
if (Test-Path $modeFile) { $modeName = (Get-Content $modeFile -Raw).Trim() }
$night = ($modeName -eq 'night')
if ($night) { $StopAt = 1.5; $label = 'night' } else { $StopAt = 0.0; $label = 'day' }

$NightRules = 'NIGHT MODE: the user is asleep and cannot test, answer or approve anything. Never ask ' +
              'or wait for them. Put everything that needs them (in-game checks, debugger sittings, ' +
              'decisions, approvals) in docs/FOR_USER.md, one short line each: what, why, exact ' +
              'command; this replaces the HANDOFF in-game list for tonight. Also keep its "Done ' +
              'tonight" list current: each item finished, merged or settled, one line with commit and ' +
              'how to see it, so the user can read the night in one place. Then move on to work that ' +
              'does not need them. If a decision cannot wait, take the safe, reversible option and ' +
              'list it there to confirm. All CLAUDE.md rules still apply, including token discipline. ' +
              "The session is stopped automatically, strictly below this block's usage cap. When told to " +
              'wrap up, do it at once: first schedule the resume ONLY if the notice names one (the ' +
              '5-hour window filled while the block has room) and write its time in ' +
              'docs/FOR_USER.md; then no new work, commit and push finished work, update ' +
              'docs/HANDOFF.md with where to resume. A stop on the block cap gets no resume: the ' +
              'user starts the next session (user rule, 2026-10-07).'

$ResumePrompt = 'Scheduled resume (night mode). Run: powershell -NoProfile -ExecutionPolicy Bypass -File ' +
                'tools/usage_budget.ps1 check. If it prints WAIT, call CronCreate with the cron it prints, ' +
                'recurring false, and this same prompt, then stop. If it prints STOP, schedule nothing and ' +
                'stop. If it prints ALLOWED, follow the CLAUDE.md ' +
                'session start and continue the work in docs/HANDOFF.md.'

function Now-Sec { [DateTimeOffset]::Now.ToUnixTimeSeconds() }
function From-Unix([double]$t) { [DateTimeOffset]::FromUnixTimeSeconds([long]$t).LocalDateTime }

# The current block: the weekly reading when it opened and when. Opens a new block from reading $r
# when there is none or the last one opened $BlockGapSec or more ago.
function Get-Block($r) {
    $b = $null
    if (Test-Path $blockFile) { $b = Get-Content $blockFile -Raw | ConvertFrom-Json }
    if ($null -eq $b -or ((Now-Sec) - [long]$b.time) -ge $BlockGapSec) {
        $b = [pscustomobject]@{ week = [double]$r.week; time = (Now-Sec) }
        $b | ConvertTo-Json -Compress | Set-Content -Path $blockFile -Encoding ASCII
    }
    $b
}

# The block's cap, stop and wrap-up points, and when the next block can open.
function Get-Schedule($r) {
    $b = Get-Block $r
    $cap = [math]::Min(100.0, [double]$b.week + $BlockPercent)
    [pscustomobject]@{ cap = $cap; stop = $cap - $StopAt; wrap = $cap - $StopAt - $WrapAt;
                       next = From-Unix ([double]$b.time + $BlockGapSec) }
}

# The verdict for a reading: ok, wrap or stop, and when the binding limit next refreshes.
function Get-State($r) {
    $s = Get-Schedule $r
    $h5 = 0.0
    if ($null -ne $r.h5) { $h5 = [double]$r.h5 }
    $weekWrap = ([double]$r.week -ge $s.wrap)
    $h5Wrap = ($night -and $h5 -ge $H5Wrap -and $null -ne $r.h5_resets)
    $state = 'ok'
    if ($weekWrap -or $h5Wrap) { $state = 'wrap' }
    if (([double]$r.week -ge $s.stop) -or ($night -and $h5 -ge $H5Stop)) { $state = 'stop' }

    # A resume only when the 5-hour window is the binding limit: reaching the block cap is
    # final (user rule, 2026-10-07).
    $resume = $null
    if ($h5Wrap -and -not $weekWrap) {
        $resume = From-Unix ([double]$r.h5_resets)
    }
    $cron = $null
    if ($null -ne $resume) {
        $resume = $resume.AddMinutes(3)
        $cron = '{0} {1} {2} {3} *' -f $resume.Minute, $resume.Hour, $resume.Day, $resume.Month
    }
    [pscustomobject]@{ s = $s; state = $state; resume = $resume; cron = $cron }
}
function Read-Rate {
    if (-not (Test-Path $rateFile)) { return $null }
    Get-Content $rateFile -Raw | ConvertFrom-Json
}

function Stop-Session([string]$msg) {
    [pscustomobject]@{ continue = $false; stopReason = $msg } | ConvertTo-Json -Compress
    exit 0
}

try {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }

    if ($Mode -eq 'status') {
        $j = [Console]::In.ReadToEnd() | ConvertFrom-Json
        $rl = $j.rate_limits
        # tools/usage_sim.ps1 writes sim.json to replace the real readings while testing.
        $simFile = Join-Path $dir 'sim.json'
        if (Test-Path $simFile) {
            $sim = Get-Content $simFile -Raw | ConvertFrom-Json
            $rl = [pscustomobject]@{
                seven_day = [pscustomobject]@{ used_percentage = $sim.week; resets_at = $sim.resets_at }
                five_hour = [pscustomobject]@{ used_percentage = $sim.h5; resets_at = $sim.h5_resets }
            }
            $label += ' SIMULATED'
        }
        if ($null -eq $rl -or $null -eq $rl.seven_day) { Write-Output "[$label] usage: waiting for first reply"; exit 0 }
        $rec = [ordered]@{ week = [double]$rl.seven_day.used_percentage; resets_at = [double]$rl.seven_day.resets_at; time = (Now-Sec) }
        if ($null -ne $rl.five_hour) { $rec.h5 = [double]$rl.five_hour.used_percentage; $rec.h5_resets = [double]$rl.five_hour.resets_at }
        [pscustomobject]$rec | ConvertTo-Json -Compress | Set-Content -Path $rateFile -Encoding ASCII
        $s = Get-Schedule ([pscustomobject]$rec)
        $line = '[{0}] week {1:N1}% of {2:N1}% allowed this block' -f $label, $rec.week, $s.cap
        if ($null -ne $rec.h5) { $line += (' | 5h {0:N0}%' -f $rec.h5) }
        Write-Output $line
        exit 0
    }

    if ($Mode -eq 'start') {
        if ($night) {
            [pscustomobject]@{
                hookSpecificOutput = [pscustomobject]@{ hookEventName = 'SessionStart'; additionalContext = $NightRules }
            } | ConvertTo-Json -Compress -Depth 3
        }
        exit 0
    }

    if ($Mode -eq 'mode') {
        $new = "$Arg".Trim().ToLower()
        if ($new -ne 'day' -and $new -ne 'night') { Write-Output "Mode is $label. Use: mode day | mode night"; exit 0 }
        Set-Content -Path $modeFile -Value $new -Encoding ASCII
        Remove-Item $warnFile -ErrorAction SilentlyContinue
        if ($new -eq 'night') { Write-Output ('Switched to night mode. ' + $NightRules) }
        else { Write-Output 'Switched to day mode: the user is around again; normal CLAUDE.md rules, in-game checks go to HANDOFF.' }
        exit 0
    }

    if ($Mode -eq 'check') {
        $r = Read-Rate
        if ($null -eq $r) { Write-Output 'ALLOWED (no usage reading yet)'; exit 0 }
        $st = Get-State $r
        if ($st.state -eq 'ok') { Write-Output ('ALLOWED: week {0:N1}%, this block allows {1:N1}%' -f [double]$r.week, $st.s.cap); exit 0 }
        if ([double]$r.week -ge $st.s.wrap) {
            Write-Output ('STOP: this block''s allowance is reached (week {0:N1}%, the block allows {1:N1}%). Schedule no resume; the user starts the next session (next block from {2:ddd HH:mm}).' -f [double]$r.week, $st.s.cap, $st.s.next)
            exit 0
        }
        $cron = $st.cron
        if ($null -eq $cron) { $t = (Get-Date).AddMinutes(63); $cron = '{0} {1} {2} {3} *' -f $t.Minute, $t.Hour, $t.Day, $t.Month }
        Write-Output ('WAIT: the 5-hour window has not refreshed yet. Schedule the resume with cron "{0}".' -f $cron)
        exit 0
    }

    if ($Mode -eq 'post') {
        $j = [Console]::In.ReadToEnd() | ConvertFrom-Json
        # Never stop on the budget check itself: Claude must still be able to schedule the resume.
        if ("$($j.tool_input.command)" -like '*usage_budget.ps1*') { exit 0 }
        $r = Read-Rate
        if ($null -eq $r) { exit 0 }
        $st = Get-State $r
        $age = (Now-Sec) - [long]$r.time
        if ($age -gt $StaleSec) {
            # Far below the wrap-up point, 30 minutes of work cannot reach the cap, so keep going.
            if ($night -and [double]$r.week -ge ($st.s.wrap - 3.0)) {
                Stop-Session ('Night mode: the usage reading is {0} minutes old and was near the cap, so staying under it cannot be guaranteed. Stopping.' -f [int]($age / 60))
            }
            exit 0
        }

        if ($st.state -eq 'stop') {
            if ([double]$r.week -ge $st.s.stop) {
                Stop-Session ('Usage schedule ({0} mode): week {1:N1}%, this block allows {2:N1}% (next block from {3:ddd HH:mm}). Stopping.' -f $label, [double]$r.week, $st.s.cap, $st.s.next)
            }
            Stop-Session ('Night mode: the 5-hour window is at {0:N0}%. Stopping until it resets.' -f [double]$r.h5)
        }

        if ($st.state -eq 'wrap') {
            $last = 0
            if (Test-Path $warnFile) { $last = [long](Get-Content $warnFile -Raw) }
            if ((Now-Sec) - $last -ge $WarnEverySec) {
                Set-Content -Path $warnFile -Value (Now-Sec) -Encoding ASCII
                $ctx = ('Usage schedule ({0} mode): week {1:N1}%, the session stops at {2:N1}%. ' -f $label, [double]$r.week, $st.s.stop)
                if ($night -and $null -ne $st.cron) {
                    # The 5-hour window filled first: resume at its refresh (user rule, 2026-10-07).
                    $ctx += ('The 5-hour window refreshes at {0:HH:mm}. FIRST, unless already done, call CronCreate with cron "{1}", recurring false, prompt: "{2}" and write that resume time in docs/FOR_USER.md. ' -f $st.resume, $st.cron, $ResumePrompt)
                } else {
                    $ctx += 'The block cap is the limit: schedule no resume. '
                }
                $ctx += 'Start no new work. Finish the current item if it is nearly done, otherwise leave it ' +
                        'unmerged. Commit and push finished work, update docs/HANDOFF.md with where to resume, ' +
                        'and tell workers to do the same.'
                [pscustomobject]@{
                    hookSpecificOutput = [pscustomobject]@{ hookEventName = 'PostToolUse'; additionalContext = $ctx }
                } | ConvertTo-Json -Compress -Depth 3
            }
        }
        exit 0
    }
} catch {
    exit 0
}
exit 0
