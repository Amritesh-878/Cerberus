Set-StrictMode -Version 2.0
. (Join-Path $PSScriptRoot 'tree.ps1')

function New-Proc([int]$Id, [int]$Parent, [string]$Name, [int]$StartedSec = $Id) {
    [pscustomobject]@{ ProcessId = $Id; ParentProcessId = $Parent; Name = $Name; CreationDate = ([datetime]'2026-01-01').AddSeconds($StartedSec) }
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

Describe 'sample.ps1' {
    It 'samples a process with no children' {
        # smss.exe runs on every Windows install and starts no lasting children.
        { & (Join-Path $PSScriptRoot 'sample.ps1') -Name smss -Seconds 1 | Out-Null } | Should Not Throw
    }

    It 'refuses an interval longer than the run' {
        { & (Join-Path $PSScriptRoot 'sample.ps1') -Name smss -Seconds 1 -Interval 2 } | Should Throw
    }
}
