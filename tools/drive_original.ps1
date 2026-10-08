# Drives the original Napoleon: Total War for side-by-side checks: launch it, click, press keys,
# take screenshots of its window. Windowed mode is assumed (preferences.script.txt
# `gfx_fullscreen false`). It takes over the mouse and keyboard, so only run it when the user
# has agreed and left the PC alone.
#
#   drive_original.ps1 launch              start the game from the install folder (refuses if one is running)
#   drive_original.ps1 shot <file.png>     screenshot of the game's client area
#   drive_original.ps1 click <x> <y> [right|double]   click at client coordinates
#   drive_original.ps1 move <x> <y>        move the mouse (hover) at client coordinates
#   drive_original.ps1 key <name> [count]  press a key (esc, enter, space, tab, f1..f12, a..z, 0..9, up, down, left, right)
#   drive_original.ps1 wait <seconds>      wait (max 120)
#   drive_original.ps1 quit                end the game this script launched
param([Parameter(Mandatory)][string]$Cmd, [string]$A, [string]$B, [string]$C)
$ErrorActionPreference = 'Stop'
$Install = 'C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War'
$PidFile = Join-Path $env:TEMP 'ntw_drive_original.pid'

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NtwDrive {
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint data, flags, time; public IntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort vk, scan; public uint flags, time; public IntPtr extra; public uint pad1, pad2; }
    [StructLayout(LayoutKind.Explicit)] struct INPUT {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public MOUSEINPUT mi;
        [FieldOffset(8)] public KEYBDINPUT ki;
    }
    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint mapType);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
    // Down and up as separate events with a pause: the game polls the buttons once a frame, so a
    // press and release in the same frame is missed by some components (tabs, faction flags).
    public static void Mouse(uint down, uint up) {
        var i = new INPUT[1];
        i[0].type = 0; i[0].mi.flags = down;
        SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
        System.Threading.Thread.Sleep(90);
        i[0].mi.flags = up;
        SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
    }
    // Scan codes, because the game reads the keyboard through DirectInput.
    public static void Key(ushort vk, bool extended) {
        ushort scan = (ushort)MapVirtualKey(vk, 0);
        uint ext = extended ? 1u : 0u;
        var i = new INPUT[2];
        i[0].type = 1; i[0].ki.scan = scan; i[0].ki.flags = 8u | ext;
        i[1].type = 1; i[1].ki.scan = scan; i[1].ki.flags = 8u | 2u | ext;
        SendInput(2, i, Marshal.SizeOf(typeof(INPUT)));
    }
}
'@

function Get-Game {
    $p = Get-Process Napoleon -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if (-not $p) { throw 'Napoleon.exe is not running (or has no window yet).' }
    $p
}

function Focus-Game($p) {
    [NtwDrive]::ShowWindow($p.MainWindowHandle, 9) | Out-Null # SW_RESTORE
    [NtwDrive]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
    # A 1920x1080 client under a title bar runs off the bottom of a 1080-line screen: move the
    # window up so the client starts at the screen's top-left corner.
    $o = To-Screen $p 0 0
    if ($o.X -ne 0 -or $o.Y -ne 0) {
        $w = New-Object NtwDrive+RECT
        [NtwDrive]::GetWindowRect($p.MainWindowHandle, [ref]$w) | Out-Null
        [NtwDrive]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, $w.L - $o.X, $w.T - $o.Y, 0, 0, 0x0001 -bor 0x0004) | Out-Null # NOSIZE | NOZORDER
    }
    Start-Sleep -Milliseconds 150
}

function To-Screen($p, [int]$x, [int]$y) {
    $pt = New-Object NtwDrive+POINT
    $pt.X = $x; $pt.Y = $y
    [NtwDrive]::ClientToScreen($p.MainWindowHandle, [ref]$pt) | Out-Null
    $pt
}

switch ($Cmd) {
    'launch' {
        if (Get-Process Napoleon -ErrorAction SilentlyContinue) { throw 'Napoleon.exe is already running; not launching a second one.' }
        # Through Steam (app 34030): started directly, the exe dies at start-up with heap
        # corruption (0xc0000374 in ntdll, 2026-10-07).
        Start-Process 'steam://rungameid/34030'
        $deadline = (Get-Date).AddSeconds(120)
        while ((Get-Date) -lt $deadline) {
            $g = Get-Process Napoleon -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
            if ($g) { Set-Content -Path $PidFile -Value $g.Id; "Launched, pid $($g.Id)"; return }
            Start-Sleep -Seconds 2
        }
        throw 'No game window after 120 s.'
    }
    'shot' {
        $p = Get-Game
        Focus-Game $p # the capture copies the screen, so the game must be on top
        Start-Sleep -Milliseconds 300
        $r = New-Object NtwDrive+RECT
        [NtwDrive]::GetClientRect($p.MainWindowHandle, [ref]$r) | Out-Null
        $o = To-Screen $p 0 0
        $w = $r.R - $r.L; $h = $r.B - $r.T
        $bmp = New-Object System.Drawing.Bitmap $w, $h
        $gr = [System.Drawing.Graphics]::FromImage($bmp)
        $gr.CopyFromScreen($o.X, $o.Y, 0, 0, $bmp.Size)
        $out = [System.IO.Path]::GetFullPath($A)
        New-Item -ItemType Directory -Force -Path (Split-Path $out) | Out-Null
        $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
        $gr.Dispose(); $bmp.Dispose()
        "Saved $out (${w}x$h)"
    }
    { $_ -in 'click', 'move' } {
        $p = Get-Game
        Focus-Game $p
        $pt = To-Screen $p ([int]$A) ([int]$B)
        # Hover first (nudge, then settle) so the component under the cursor sees the mouse enter.
        [NtwDrive]::SetCursorPos($pt.X - 4, $pt.Y - 4) | Out-Null
        Start-Sleep -Milliseconds 150
        [NtwDrive]::SetCursorPos($pt.X, $pt.Y) | Out-Null
        Start-Sleep -Milliseconds 350
        if ($Cmd -eq 'click') {
            $n = if ($C -eq 'double') { 2 } else { 1 }
            for ($i = 0; $i -lt $n; $i++) {
                if ($C -eq 'right') { [NtwDrive]::Mouse(0x0008, 0x0010) } else { [NtwDrive]::Mouse(0x0002, 0x0004) }
                Start-Sleep -Milliseconds 80
            }
        }
        "$Cmd at $A,$B"
    }
    'key' {
        $p = Get-Game
        Focus-Game $p
        $names = @{ esc = 0x1B; enter = 0x0D; space = 0x20; tab = 0x09; up = 0x26; down = 0x28; left = 0x25; right = 0x27; back = 0x08 }
        $k = $A.ToLower()
        $ext = $k -in 'up', 'down', 'left', 'right'
        if ($names.ContainsKey($k)) { $vk = $names[$k] }
        elseif ($k -match '^f(\d{1,2})$') { $vk = 0x6F + [int]$Matches[1] }
        elseif ($k -match '^[a-z0-9]$') { $vk = [int][char]$k.ToUpper() }
        else { throw "Unknown key $A" }
        $count = if ($B) { [int]$B } else { 1 }
        for ($i = 0; $i -lt $count; $i++) { [NtwDrive]::Key([uint16]$vk, $ext); Start-Sleep -Milliseconds 150 }
        "key $A x$count"
    }
    'wait' {
        $s = [math]::Min([double]$A, 120)
        Start-Sleep -Seconds $s
        "waited $s s"
    }
    'quit' {
        if (-not (Test-Path $PidFile)) { throw 'No game launched by this script.' }
        $id = [int](Get-Content $PidFile)
        $g = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($g -and $g.ProcessName -eq 'Napoleon') { Stop-Process -Id $id; "Ended pid $id" } else { 'Already gone' }
        Remove-Item $PidFile
    }
    default { throw "Unknown command $Cmd" }
}
