param(
    [Parameter(Mandatory)] [string] $Kando,
    [string] $Orbit = (Join-Path $env:LOCALAPPDATA "Orbit Actions"),
    [string] $Out = (Join-Path $PSScriptRoot "..\docs\results.json"),
    [int] $Runs = 5
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class Win {
    delegate bool EnumProc(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr p);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int L, T, R, B; }
    public static int Visible(int[] pids) {
        var set = new HashSet<uint>();
        foreach (var p in pids) set.Add((uint)p);
        int count = 0;
        EnumWindows((h, _) => {
            uint pid;
            GetWindowThreadProcessId(h, out pid);
            RECT r;
            if (set.Contains(pid) && IsWindowVisible(h) && GetWindowRect(h, out r) && r.R - r.L > 50 && r.B - r.T > 50) count++;
            return true;
        }, IntPtr.Zero);
        return count;
    }
    public static void Escape() {
        keybd_event(0x1B, 0, 0, UIntPtr.Zero);
        keybd_event(0x1B, 0, 2, UIntPtr.Zero);
    }
}
"@

$ipc = Join-Path $env:APPDATA "kando\ipc-info.json"
$apps = [ordered]@{
    "Orbit Actions" = @{ Exe = Join-Path $Orbit "orbit-actions.exe"; Names = @("orbit-actions", "orbit-actions-settings"); Folder = $Orbit; Files = @("orbit-actions.exe", "orbit-actions-settings.exe") }
    "Kando 3.0.1" = @{ Exe = Join-Path $Kando "kando.exe"; Names = @("kando"); Folder = $Kando; Files = $null }
}
$menuName = (Get-Content (Join-Path $Orbit "menus.json") -Raw -Encoding UTF8 | ConvertFrom-Json).menus[0].root.name

function Tree($names) {
    @(Get-Process -Name $names -ErrorAction SilentlyContinue)
}

function Snapshot($names) {
    $list = Tree $names
    [ordered]@{
        processes = $list.Count
        workingSetMB = [math]::Round(($list | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
        privateMB = [math]::Round(($list | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
        threads = ($list | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum
        handles = ($list | Measure-Object HandleCount -Sum).Sum
    }
}

function CpuSeconds($names) {
    ((Tree $names) | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum
}

function StopAll {
    foreach ($app in $apps.Values) {
        Tree $app.Names | Stop-Process -Force -ErrorAction SilentlyContinue
    }
    for ($i = 0; $i -lt 100; $i++) {
        if (-not ($apps.Values | ForEach-Object { Tree $_.Names })) { break }
        Start-Sleep -Milliseconds 100
    }
    Remove-Item $ipc -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

function Ready {
    if (-not (Test-Path $ipc)) { return $false }
    try {
        $port = (Get-Content $ipc -Raw | ConvertFrom-Json).port
        $client = New-Object Net.Sockets.TcpClient
        $client.Connect("127.0.0.1", $port)
        $client.Close()
        return $true
    } catch { return $false }
}

function Launch($app) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Process $app.Exe -WorkingDirectory (Split-Path $app.Exe) | Out-Null
    while (-not (Ready)) {
        if ($clock.Elapsed.TotalSeconds -gt 60) { throw "$($app.Exe) did not start" }
        Start-Sleep -Milliseconds 5
    }
    $clock.Elapsed.TotalMilliseconds
}

function OpenMenu($app) {
    $pids = (Tree $app.Names).Id
    $before = [Win]::Visible($pids)
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Process $app.Exe -ArgumentList "--menu", "`"$menuName`"" | Out-Null
    while ([Win]::Visible((Tree $app.Names).Id) -le $before) {
        if ($clock.Elapsed.TotalSeconds -gt 10) { return $null }
        Start-Sleep -Milliseconds 2
    }
    $clock.Elapsed.TotalMilliseconds
}

function Median($values) {
    $sorted = @($values | Where-Object { $_ -ne $null } | Sort-Object)
    if (-not $sorted) { return $null }
    [math]::Round($sorted[[int][math]::Floor($sorted.Count / 2)], 0)
}

$results = [ordered]@{
    date = (Get-Date).ToString("yyyy-MM-dd HH:mm")
    machine = [ordered]@{
        cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
        ramGB = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
        os = (Get-CimInstance Win32_OperatingSystem).Caption + " " + [Environment]::OSVersion.Version
    }
    runs = $Runs
    apps = [ordered]@{}
}

foreach ($name in $apps.Keys) {
    $app = $apps[$name]
    if ($app.Files) {
        $files = $app.Files | ForEach-Object { Get-Item (Join-Path $app.Folder $_) }
    } else {
        $files = Get-ChildItem $app.Folder -Recurse -File
    }
    $entry = [ordered]@{
        diskMB = [math]::Round(($files | Measure-Object Length -Sum).Sum / 1MB, 1)
        files = @($files).Count
    }

    $starts = @()
    $menus = @()
    for ($run = 0; $run -lt $Runs; $run++) {
        StopAll
        $starts += Launch $app
        Start-Sleep -Seconds 3
        $menus += OpenMenu $app
        Start-Sleep -Milliseconds 500
        [Win]::Escape()
    }
    $entry.startMs = Median $starts
    $entry.startRunsMs = @($starts | ForEach-Object { [math]::Round($_, 0) })
    $entry.menuOpenMs = Median $menus
    $entry.menuOpenRunsMs = @($menus | ForEach-Object { if ($_ -ne $null) { [math]::Round($_, 0) } })

    StopAll
    Launch $app | Out-Null
    Start-Sleep -Seconds 20
    $entry.idle = Snapshot $app.Names
    $cpu = CpuSeconds $app.Names
    Start-Sleep -Seconds 60
    $entry.idleCpuPercent = [math]::Round(((CpuSeconds $app.Names) - $cpu) / 60 / [Environment]::ProcessorCount * 100, 3)

    $cpu = CpuSeconds $app.Names
    OpenMenu $app | Out-Null
    Start-Sleep -Seconds 2
    $entry.menuOpen = Snapshot $app.Names
    $entry.menuOpenCpuSeconds = [math]::Round((CpuSeconds $app.Names) - $cpu, 2)
    [Win]::Escape()
    Start-Sleep -Seconds 1

    for ($i = 0; $i -lt 20; $i++) {
        OpenMenu $app | Out-Null
        Start-Sleep -Milliseconds 400
        [Win]::Escape()
        Start-Sleep -Milliseconds 400
    }
    Start-Sleep -Seconds 5
    $entry.after20Menus = Snapshot $app.Names

    Start-Process $app.Exe -ArgumentList "--settings" | Out-Null
    Start-Sleep -Seconds 8
    $entry.settingsOpen = Snapshot $app.Names

    $results.apps[$name] = $entry
}

StopAll
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$results | ConvertTo-Json -Depth 6 | Out-File $Out -Encoding utf8
