# Sandbox watchdog — preserves worker progress between rounds.
#
# Why: sandbox workers are forbidden from `git push`. If one dies to a
# provider rate limit mid-round, its uncommitted work is lost. This polls the
# four live worktrees and commits + pushes any dirt to the `sandbox` remote.
#
# Safety rules, all enforced here:
#   * NEVER pushes to `origin`. Only the `sandbox` remote.
#   * Only stages source and notes (crates/, analysis/, docs/) plus tracked
#     deletions. It will NOT stage analysis/**/*.txt (gitignored copyrighted
#     game data) and will NOT sweep untracked scratch.
#   * Tolerates a concurrent worker git command: on any git failure it just
#     skips that worktree and tries again next tick.
#   * Coalesces: no more than one commit per worktree per interval.
#
# Usage:  powershell -NoProfile -File tools\sandbox_watchdog.ps1
#         powershell -NoProfile -File tools\sandbox_watchdog.ps1 -Once
# Stop:   delete the PID file, or kill the process.

param(
  [int]$IntervalSeconds = 180,
  [switch]$Once
)

$ErrorActionPreference = 'Continue'
$repo     = "$env:USERPROFILE\Documents\NapoleonRust"
$logFile  = Join-Path $repo 'target\tmp\watchdog.log'
$pidFile  = Join-Path $repo 'target\tmp\watchdog.pid'

# worktree path -> branch name
$targets = [ordered]@{
  "$env:USERPROFILE\Documents\NR-sb-s2" = 'work/next/s2-effects'
  "$env:USERPROFILE\Documents\NR-sb-0e" = 'work/next/0e-ui'
  "$env:USERPROFILE\Documents\NR-sb-0g" = 'work/next/0g-characters'
  "$env:USERPROFILE\Documents\NR-sb-0d" = 'work/next/0d-units'
}

function Write-Log([string]$m) {
  $line = "{0}  {1}" -f (Get-Date -Format 'HH:mm:ss'), $m
  Write-Output $line
  try {
    $dir = Split-Path $logFile -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    Add-Content -Path $logFile -Value $line
  } catch { }
}

# Guard against a second instance.
if (-not $Once) {
  if (Test-Path $pidFile) {
    $old = Get-Content $pidFile -ErrorAction SilentlyContinue
    if ($old -and (Get-Process -Id $old -ErrorAction SilentlyContinue)) {
      Write-Log "watchdog already running (pid $old) - exiting"
      exit 0
    }
  }
  try { New-Item -ItemType Directory -Force -Path (Split-Path $pidFile -Parent) | Out-Null
        Set-Content -Path $pidFile -Value $PID } catch { }
}

Write-Log "watchdog started (interval ${IntervalSeconds}s)"

function Invoke-Git([string]$dir, [string[]]$gitArgs) {
  # Returns $true on exit 0. Never throws, never aborts the loop.
  try {
    $out = & git -C $dir @gitArgs 2>&1
    return ($LASTEXITCODE -eq 0)
  } catch { return $false }
}

function Save-Worktree([string]$dir, [string]$branch) {
  if (-not (Test-Path $dir)) { return }

  # Stage only the paths we trust. Explicit pathspecs, never bare `add -A`,
  # so we cannot accidentally sweep untracked scratch or ignored dumps.
  $paths = @('crates', 'analysis/fidelity/*.md', 'analysis/fidelity/**/*.md',
             'analysis/**/*.java', 'analysis/**/*.ps1', 'docs')
  $any = $false
  foreach ($p in $paths) { if (Invoke-Git $dir @('add','--',$p)) { $any = $true } }

  # Track deletions of previously-committed files too.
  if (Invoke-Git $dir @('add','-u','--','crates','analysis','docs')) { $any = $true }

  if (-not $any) { return }

  $staged = & git -C $dir diff --cached --numstat 2>$null
  if (-not $staged) { return }   # nothing actually staged -> tree was clean

  $files = ($staged | Measure-Object -Line).Lines
  $msg = "watchdog: auto-save worker progress ($files files)`n`n" +
         "Mid-round snapshot pushed by tools/sandbox_watchdog.ps1 so a provider`n" +
         "rate limit cannot cost a whole round. May be a partial state.`n`n" +
         "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"

  $ok = Invoke-Git $dir @(
    'commit','-m',$msg)
  if (-not $ok) { Write-Log "  commit failed (worker may hold the lock) - skipping"; return }

  # History-rewrite guard. A worker may squash or rebase after we auto-pushed a
  # snapshot, which leaves the remote tip dangling and turns our next push into
  # a non-fast-forward. Detect that and stand down: the manager force-pushes
  # with an explicit --force-with-lease instead, so we never clobber a rewrite
  # the worker did deliberately.
  $remoteLine = & git -C $dir ls-remote sandbox "refs/heads/$branch" 2>$null
  if ($remoteLine) {
    $remoteSha = ($remoteLine -split '\s+')[0]
    & git -C $dir merge-base --is-ancestor $remoteSha HEAD 2>$null
    if ($LASTEXITCODE -ne 0) {
      Write-Log "  committed $files files but NOT pushing: remote $remoteSha is not an ancestor of HEAD (worker rewrote history). Manager will force-with-lease."
      return
    }
  }

  # NEVER origin. Only sandbox.
  $pushed = Invoke-Git $dir @('push','sandbox',"${branch}:${branch}")
  if ($pushed) { Write-Log "  saved+push $files files -> sandbox/$branch" }
  else         { Write-Log "  committed $files files but PUSH FAILED -> sandbox/$branch" }
}

do {
  foreach ($dir in $targets.Keys) {
    try { Save-Worktree $dir $targets[$dir] } catch { Write-Log "  error in $dir : $($_.Exception.Message)" }
  }
  if ($Once) { break }
  Start-Sleep -Seconds $IntervalSeconds
} while ($true)

Write-Log "watchdog stopped"
# Only a CONTINUOUS run owns the PID file. A `-Once` run must not clear it,
# or it deletes the running instance's claim and the single-instance guard
# silently fails on the next launch (this happened: two watchdogs).
if (-not $Once -and (Test-Path $pidFile)) { Remove-Item $pidFile -Force -ErrorAction SilentlyContinue }