# Written for Pester 3.4.0, which ships with Windows PowerShell 5.1; Pester 5 dropped this assertion syntax.
Set-StrictMode -Version 2.0
. (Join-Path $PSScriptRoot 'core.ps1')

function New-Proc([int]$Id, [int]$Parent, [string]$Name, [int]$StartedSec = $Id) {
    [pscustomobject]@{ ProcessId = $Id; ParentProcessId = $Parent; Name = $Name; CreationDate = ([datetime]'2026-01-01').AddSeconds($StartedSec) }
}

function New-Row([string]$Time, [string]$Group, [string]$Name, [double]$WsMb) {
    [pscustomobject]@{ Time = $Time; Group = $Group; Name = $Name; Count = 1; Cpu = 0.5; WsMb = $WsMb; PrivMb = $WsMb }
}

Describe 'Get-TreeRow' {
    It 'counts a same-named process under a differently named parent once' {
        $snapshot = @(
            (New-Proc 1 0 'node.exe'), (New-Proc 2 1 'cmd.exe'), (New-Proc 3 2 'node.exe'), (New-Proc 4 3 'git.exe')
        )
        $tree = Get-TreeRow -Snapshot $snapshot -RootName 'node' -Skip @()
        $tree.Count | Should Be 4
        @($tree | ForEach-Object { $_.Process.ProcessId } | Sort-Object -Unique).Count | Should Be 4
    }

    It 'returns an array for a tree of one process' {
        $tree = Get-TreeRow -Snapshot @((New-Proc 7 0 'lone.exe')) -RootName 'lone' -Skip @()
        $tree.Count | Should Be 1
        $tree[0].Group | Should Be 'app'
    }

    It 'puts the app and its pty hosts in app and the rest in child' {
        $snapshot = @(
            (New-Proc 1 0 'term.exe'), (New-Proc 2 1 'openconsole.exe'), (New-Proc 3 1 'powershell.exe'), (New-Proc 4 3 'conhost.exe')
        )
        $groups = @{}
        foreach ($r in (Get-TreeRow -Snapshot $snapshot -RootName 'term' -Skip @())) { $groups[$r.Process.Name] = $r.Group }
        $groups['openconsole.exe'] | Should Be 'app'
        $groups['powershell.exe'] | Should Be 'child'
        $groups['conhost.exe'] | Should Be 'child'
    }

    It 'skips a child whose PID was reused by a process older than its parent' {
        $snapshot = @((New-Proc 5 0 'term.exe' 10), (New-Proc 6 5 'stale.exe' 1))
        (Get-TreeRow -Snapshot $snapshot -RootName 'term' -Skip @()).Count | Should Be 1
    }
}

Describe 'Get-SampleStamp' {
    It 'keeps two samples taken in the same second apart' {
        $first = Get-SampleStamp ([datetime]'2026-01-01T10:00:01.200')
        $second = Get-SampleStamp ([datetime]'2026-01-01T10:00:01.700')
        $first | Should Not Be $second
        $rows = @((New-Row $first 'app' 'term.exe' 100), (New-Row $second 'app' 'term.exe' 100))
        (Write-Summary $rows 'test cores')[1] | Should Be 'Summary over 2 samples (CPU % of test cores)'
    }
}

Describe 'Format-SampleLine' {
    It 'prints a sample that has no children' {
        $line = Format-SampleLine -Rows @((New-Row 't' 'app' 'lone.exe' 3)) -Clock '10:00:01' -ProcessCount 1
        $line | Should Match '^10:00:01 +1 \| +0\.5 +3 +3 \| +0\.0 +0 +0 \|'
    }
}

Describe 'sample.ps1' {
    It 'refuses an interval longer than the run' {
        { & (Join-Path $PSScriptRoot 'sample.ps1') -Name term -Seconds 1 -Interval 2 } | Should Throw
    }

    It 'leaves no csv behind when the process is not running' {
        $csv = Join-Path $TestDrive 'run.csv'
        { & (Join-Path $PSScriptRoot 'sample.ps1') -Name 'no-such-process-for-bench' -Seconds 1 -Csv $csv -Quiet } | Should Throw
        Test-Path $csv | Should Be $false
    }

    It 'refuses an existing csv whose name has wildcard characters' {
        $csv = Join-Path $TestDrive 'run[1].csv'
        Set-Content -LiteralPath $csv -Value 'kept'
        { & (Join-Path $PSScriptRoot 'sample.ps1') -Name 'no-such-process-for-bench' -Seconds 1 -Csv $csv -Quiet } | Should Throw 'exists'
    }
}
