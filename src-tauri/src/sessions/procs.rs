/// What the system says about processes, as plain functions so tests can stand in for it.
pub struct Probes {
    pub below: fn(u32, u64) -> Vec<u32>,
    pub is_running: fn(u32) -> bool,
    /// Whether a process runs under that id and had started by the given Unix milliseconds, if known.
    pub running_by: fn(u32, Option<u64>) -> bool,
    /// Ends a process, but only one started at or after the given Unix milliseconds.
    pub stop: fn(u32, u64),
}

impl Probes {
    pub const SYSTEM: Self = Self {
        below: descendants,
        is_running,
        running_by,
        stop: stop_since,
    };
}

/// Ends `pid` if it started at or after `since_ms`, so a reused id of an older, unrelated process is never touched.
#[cfg(windows)]
pub fn stop_since(pid: u32, since_ms: u64) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, TerminateProcess,
    };

    // SAFETY: a handle this function opens, passes on and closes.
    unsafe {
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            0,
            pid,
        );
        if process.is_null() {
            return;
        }
        if created_ms(process).is_some_and(|started| started >= since_ms) {
            TerminateProcess(process, 1);
        }
        CloseHandle(process);
    }
}

/// When the process behind `process` was created, in Unix milliseconds.
#[cfg(windows)]
fn created_ms(process: windows_sys::Win32::Foundation::HANDLE) -> Option<u64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetProcessTimes;
    // 100 ns ticks from 1601 to 1970.
    const EPOCH_TICKS: u64 = 116_444_736_000_000_000;

    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: the caller holds the handle open; the out-pointers are locals.
    let read = unsafe {
        GetProcessTimes(
            process,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    } != 0;
    let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    read.then(|| ticks.saturating_sub(EPOCH_TICKS) / 10_000)
}

/// When process `pid` was created, in Unix milliseconds, if it is running and readable.
#[cfg(windows)]
fn born_ms(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: a handle this function opens, passes on and closes.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let born = created_ms(process);
        CloseHandle(process);
        born
    }
}

/// Unix walks a tab's process group, which an unrelated process never joins, so no start time is needed.
#[cfg(unix)]
pub fn stop_since(pid: u32, _since_ms: u64) {
    if let Ok(pid) = libc::pid_t::try_from(pid) {
        // SAFETY: sends a signal and touches no memory; a gone process makes it fail harmlessly.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

/// Every process below `root`, which started at `since_ms`, children first, from Windows' process list.
#[cfg(windows)]
pub fn descendants(root: u32, since_ms: u64) -> Vec<u32> {
    use std::mem::{size_of, zeroed};

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let mut pairs = Vec::new();
    // SAFETY: a snapshot handle this function owns, walked with a struct whose size field is set as documented.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(0);
        let mut more = Process32FirstW(snapshot, &raw mut entry) != 0;
        while more {
            pairs.push((entry.th32ProcessID, entry.th32ParentProcessID));
            more = Process32NextW(snapshot, &raw mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    // The tab's start is read just before its program is created, and creation times can lag that clock by a tick.
    below(
        &pairs,
        root,
        since_ms.saturating_sub(BORN_SLACK_MS),
        born_ms,
    )
}

/// macOS moves an orphan to launchd, so a parent id is never stale and no start time is needed.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn descendants(root: u32, _since_ms: u64) -> Vec<u32> {
    let listed = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pgid="])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    match listed {
        Ok(out) if out.status.success() => {
            in_tree(&ps_rows(&String::from_utf8_lossy(&out.stdout)), root)
        }
        _ => Vec::new(),
    }
}

/// (process, parent, group) rows from `ps -o pid=,ppid=,pgid=`; a line that is not three numbers is skipped.
#[cfg(any(all(unix, not(target_os = "linux")), test))]
fn ps_rows(listing: &str) -> Vec<(u32, u32, u32)> {
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(str::parse::<u32>);
            match (fields.next(), fields.next(), fields.next(), fields.next()) {
                (Some(Ok(pid)), Some(Ok(ppid)), Some(Ok(pgid)), None) => Some((pid, ppid, pgid)),
                _ => None,
            }
        })
        .collect()
}

/// Below `root` by parent, plus its group, since an orphan moves to launchd but keeps the group it started in.
#[cfg(any(all(unix, not(target_os = "linux")), test))]
fn in_tree(rows: &[(u32, u32, u32)], root: u32) -> Vec<u32> {
    let pairs: Vec<(u32, u32)> = rows.iter().map(|&(pid, ppid, _)| (pid, ppid)).collect();
    let mut found = below(&pairs, root, 0, |_| Some(0));
    for &(pid, _, pgid) in rows {
        if pgid == root && pid != root && !found.contains(&pid) {
            found.push(pid);
        }
    }
    found
}

/// Allowed gap between a process's creation and the start time it stamps in its own session file.
const STAMP_SLACK_MS: u64 = 1_000;

/// Whether `pid` runs and had started by `at_ms`, so it may be the process that stamped a file then; a reused id starts later.
pub fn running_by(pid: u32, at_ms: Option<u64>) -> bool {
    is_running(pid)
        && match (at_ms, born_ms(pid)) {
            (Some(at), Some(born)) => born <= at.saturating_add(STAMP_SLACK_MS),
            // Without both times the running process counts, so a live chat is never copied over.
            _ => true,
        }
}

#[cfg(target_os = "macos")]
fn born_ms(pid: u32) -> Option<u64> {
    use std::mem::{size_of, zeroed};

    let pid = libc::c_int::try_from(pid).ok()?;
    let size = libc::c_int::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: a zeroed plain-data struct the call fills, with its exact size passed.
    let (read, info) = unsafe {
        let mut info: libc::proc_bsdinfo = zeroed();
        let read = libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size);
        (read, info)
    };
    (read == size).then(|| info.pbi_start_tvsec * 1_000 + info.pbi_start_tvusec / 1_000)
}

#[cfg(target_os = "linux")]
fn born_ms(pid: u32) -> Option<u64> {
    let clock = linux::Clock::read()?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    Some(clock.ms_at(parse_stat(&stat)?.start))
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn born_ms(_pid: u32) -> Option<u64> {
    None
}

/// One `/proc/<pid>/stat` line: the process, its parent, its group and its start in clock ticks since boot.
#[cfg(any(target_os = "linux", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stat {
    pid: u32,
    ppid: u32,
    pgid: u32,
    start: u64,
}

/// The command name sits in parentheses and may itself hold spaces and `)`, so fields count from the last `)`.
#[cfg(any(target_os = "linux", test))]
fn parse_stat(line: &str) -> Option<Stat> {
    let (head, rest) = line.rsplit_once(')')?;
    let pid = head.split_once('(')?.0.trim().parse().ok()?;
    // After the name: state is field 3, ppid 4, pgrp 5 and starttime 22.
    let fields: Vec<&str> = rest.split_whitespace().collect();
    Some(Stat {
        pid,
        ppid: fields.get(1)?.parse().ok()?,
        pgid: fields.get(2)?.parse().ok()?,
        start: fields.get(19)?.parse().ok()?,
    })
}

/// Below `root` by parent and by its group (which outlives a parent), none started before the root, or before `since` ticks once it is gone.
#[cfg(any(target_os = "linux", test))]
fn proc_tree(stats: &[Stat], root: u32, since: u64) -> Vec<u32> {
    let starts: std::collections::HashMap<u32, u64> =
        stats.iter().map(|s| (s.pid, s.start)).collect();
    let root_start = starts.get(&root).copied().unwrap_or(since);
    let pairs: Vec<(u32, u32)> = stats.iter().map(|s| (s.pid, s.ppid)).collect();
    let mut found = below(&pairs, root, root_start, |pid| starts.get(&pid).copied());
    for s in stats {
        if s.pgid == root && s.pid != root && s.start >= root_start && !found.contains(&s.pid) {
            found.push(s.pid);
        }
    }
    found
}

/// Every process below `root`, which started at `since_ms`, from `/proc`.
#[cfg(target_os = "linux")]
pub fn descendants(root: u32, since_ms: u64) -> Vec<u32> {
    let Some(clock) = linux::Clock::read() else {
        return Vec::new();
    };
    // `btime` is whole seconds, so a tick count from it can run up to a second late.
    let since = clock.ticks_at(since_ms.saturating_sub(1_000));
    proc_tree(&linux::stats(), root, since)
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Stat, parse_stat};

    /// Boot time and tick rate, to turn a stat line's start ticks into Unix milliseconds and back.
    pub struct Clock {
        boot_ms: u64,
        hz: u64,
    }

    impl Clock {
        pub fn read() -> Option<Self> {
            let stat = std::fs::read_to_string("/proc/stat").ok()?;
            let boot: u64 = stat
                .lines()
                .find_map(|line| line.strip_prefix("btime "))?
                .trim()
                .parse()
                .ok()?;
            // SAFETY: sysconf reads a constant and touches no memory.
            let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
            let hz = u64::try_from(hz).ok().filter(|&hz| hz > 0)?;
            Some(Self {
                boot_ms: boot.saturating_mul(1_000),
                hz,
            })
        }

        pub fn ms_at(&self, ticks: u64) -> u64 {
            self.boot_ms + ticks.saturating_mul(1_000) / self.hz
        }

        pub fn ticks_at(&self, ms: u64) -> u64 {
            ms.saturating_sub(self.boot_ms).saturating_mul(self.hz) / 1_000
        }
    }

    /// Every readable process; one that exits mid-read is simply left out.
    pub fn stats() -> Vec<Stat> {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
            })
            .filter_map(|e| std::fs::read_to_string(e.path().join("stat")).ok())
            .filter_map(|line| parse_stat(&line))
            .collect()
    }
}

/// Whether a process with this id is still running.
#[cfg(windows)]
pub fn is_running(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: a handle this function opens, reads and closes.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut code = 0;
        let read = GetExitCodeProcess(process, &raw mut code) != 0;
        CloseHandle(process);
        read && code == STILL_ACTIVE as u32
    }
}

#[cfg(unix)]
pub fn is_running(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // Signal 0 only checks; EPERM means the process exists under another user.
    // SAFETY: `kill` with signal 0 sends nothing and touches no memory.
    let sent = unsafe { libc::kill(pid, 0) };
    sent == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Allowed lag between the clock read before a spawn and the coarser clock that stamps process creation.
#[cfg(windows)]
const BORN_SLACK_MS: u64 = 100;

/// `pairs` are (process, parent); the walk is breadth first, so nearer processes come first.
/// A link counts only when `born` says the child started at or after its parent (the root at `root_born`),
/// since Windows keeps a dead parent's id and an older process may name a pid that was later reused.
fn below(
    pairs: &[(u32, u32)],
    root: u32,
    root_born: u64,
    born: impl Fn(u32) -> Option<u64>,
) -> Vec<u32> {
    let mut found = Vec::new();
    let mut next = std::collections::VecDeque::from([(root, root_born)]);
    while let Some((parent, parent_born)) = next.pop_front() {
        for &(pid, of) in pairs {
            if of != parent || pid == root || found.contains(&pid) {
                continue;
            }
            if let Some(at) = born(pid).filter(|&at| at >= parent_born) {
                found.push(pid);
                next.push_back((pid, at));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_finds_children_then_grandchildren_and_nothing_else() {
        let pairs = [(10, 1), (20, 10), (21, 10), (30, 20), (40, 2), (10, 1)];

        assert_eq!(below(&pairs, 10, 0, |_| Some(0)), [20, 21, 30]);
        assert_eq!(below(&pairs, 30, 0, |_| Some(0)), Vec::<u32>::new());
    }

    #[test]
    fn an_older_process_naming_the_roots_reused_pid_is_not_followed() {
        // 7 started at 1_000 under a parent that died; the tab's root got pid 7's old parent id, 50, at 5_000.
        let pairs = [(7, 50), (8, 7), (60, 50), (61, 60)];
        let born = |pid| match pid {
            7 => Some(1_000),
            8 => Some(6_000),
            60 => Some(5_100),
            61 => Some(5_200),
            _ => None,
        };

        assert_eq!(below(&pairs, 50, 5_000, born), [60, 61]);
    }

    #[test]
    fn a_child_older_than_its_parent_or_unreadable_is_not_followed() {
        // 21 claims parent 20 but predates it, so 20's id was reused; 22's start can't be read.
        let pairs = [(20, 10), (21, 20), (30, 21), (22, 10)];
        let born = |pid| match pid {
            20 => Some(2_000),
            21 => Some(1_500),
            30 => Some(3_000),
            _ => None,
        };

        assert_eq!(below(&pairs, 10, 1_000, born), [20]);
    }

    #[cfg(windows)]
    #[test]
    fn this_process_is_running_and_a_finished_one_is_not() {
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "exit"])
            .spawn()
            .unwrap();
        child.wait().unwrap();

        assert!(is_running(std::process::id()));
        assert!(!is_running(child.id()));
    }

    fn stat_line(pid: u32, name: &str, ppid: u32, pgid: u32, start: u64) -> String {
        // Fields 10 to 21 are zeros here; starttime is field 22, and real lines run on past it.
        format!(
            "{pid} ({name}) S {ppid} {pgid} {pgid} 0 -1 4194560 {}{start} 1 0 0",
            "0 ".repeat(12)
        )
    }

    #[test]
    fn a_stat_line_is_read_past_a_name_with_spaces_and_parentheses() {
        let odd = stat_line(4242, "my (odd) name) x", 17, 4000, 987_654);

        assert_eq!(
            parse_stat(&odd),
            Some(Stat {
                pid: 4242,
                ppid: 17,
                pgid: 4000,
                start: 987_654
            })
        );
        assert_eq!(
            parse_stat(&stat_line(1, "systemd", 0, 1, 2)).map(|s| s.start),
            Some(2)
        );
        assert_eq!(parse_stat("12 (cut short) S 1 12"), None);
        assert_eq!(parse_stat("no parentheses at all"), None);
        assert_eq!(parse_stat(""), None);
    }

    fn stat(pid: u32, ppid: u32, pgid: u32, start: u64) -> Stat {
        Stat {
            pid,
            ppid,
            pgid,
            start,
        }
    }

    #[test]
    fn the_proc_walk_follows_children_started_after_their_parent() {
        // Root 50 started at 5_000; 7 is older and names 50 only because its parent's id was reused.
        let stats = [
            stat(50, 1, 50, 5_000),
            stat(60, 50, 50, 5_100),
            stat(61, 60, 50, 5_200),
            stat(7, 50, 7, 1_000),
            stat(8, 7, 7, 6_000),
            stat(99, 1, 99, 100),
        ];

        assert_eq!(proc_tree(&stats, 50, 0), [60, 61]);
    }

    #[test]
    fn the_proc_walk_finds_an_orphan_by_the_roots_group_but_not_an_older_member() {
        // 600 outlived the tab's program 500 and now hangs off 1; 450 sits in group 500 but predates it.
        let stats = [
            stat(1, 0, 1, 0),
            stat(500, 1, 500, 5_000),
            stat(600, 1, 500, 5_500),
            stat(601, 600, 500, 5_600),
            stat(450, 1, 500, 4_000),
        ];

        assert_eq!(proc_tree(&stats, 500, 0), [600, 601]);
    }

    #[test]
    fn once_the_root_is_gone_since_bounds_the_walk() {
        let stats = [stat(600, 1, 500, 5_500), stat(300, 1, 500, 3_000)];

        assert_eq!(proc_tree(&stats, 500, 5_000), [600]);
        assert_eq!(proc_tree(&stats, 500, 0), [600, 300]);
    }

    #[test]
    fn a_loop_in_proc_parents_ends() {
        let stats = [stat(1, 2, 1, 0), stat(2, 1, 2, 0)];

        assert_eq!(proc_tree(&stats, 1, 0), [2]);
    }

    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_process_started_after_a_files_stamp_is_not_the_one_that_wrote_it() {
        let now_ms = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let me = std::process::id();

        assert!(running_by(me, Some(now_ms)));
        assert!(running_by(me, None));
        // This process started well after a stamp from an hour ago, as a reused id's process does.
        assert!(!running_by(me, Some(now_ms - 3_600_000)));
    }

    #[cfg(windows)]
    #[test]
    fn stop_ends_a_process_started_since_and_spares_an_older_one() {
        let now_ms = || {
            u64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
            )
            .unwrap()
        };
        let sleeper = || {
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", "Start-Sleep 30"])
                .spawn()
                .unwrap()
        };
        let mut old = sleeper();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let since = now_ms();
        let mut new = sleeper();
        std::thread::sleep(std::time::Duration::from_millis(300));

        stop_since(old.id(), since);
        stop_since(new.id(), since);
        new.wait().unwrap();

        assert!(!is_running(new.id()));
        assert!(is_running(old.id()));
        old.kill().unwrap();
        old.wait().unwrap();
    }

    #[test]
    fn ps_lines_become_rows_and_odd_lines_are_skipped() {
        let listing = [
            "    1     0     1",
            "  412     1   412",
            " 9000   412   412",
            "PID PPID PGID",
            "",
            " 77 x 1",
            " 5 6",
            " 5 6 7 8",
        ]
        .join("\n");

        assert_eq!(
            ps_rows(&listing),
            [(1, 0, 1), (412, 1, 412), (9000, 412, 412)]
        );
    }

    #[test]
    fn a_child_whose_parent_exited_is_still_found_by_its_group() {
        // 500 was the tab's program; 600 outlived it and now hangs off launchd (1), in 500's group.
        let rows = [
            (1, 0, 1),
            (600, 1, 500),
            (601, 600, 500),
            (700, 1, 700),
            (701, 700, 500),
        ];

        assert_eq!(in_tree(&rows, 500), [600, 601, 701]);
        assert_eq!(in_tree(&rows, 700), [701]);
    }

    #[cfg(unix)]
    #[test]
    fn a_program_left_by_a_wrapper_that_exited_is_still_found() {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;
        // A group leader, as each tab's program is; it leaves sleep running and exits.
        let mut wrapper = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30 & echo $!"])
            .stdout(std::process::Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(wrapper.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let left: u32 = line.trim().parse().unwrap();
        wrapper.wait().unwrap();

        let found = descendants(wrapper.id(), 0);
        // SAFETY: plain signal to a test-owned process.
        unsafe { libc::kill(libc::pid_t::try_from(left).unwrap(), libc::SIGKILL) };

        assert!(found.contains(&left), "{left} not in {found:?}");
    }

    #[cfg(unix)]
    #[test]
    fn this_process_is_running_and_a_finished_one_is_not() {
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        child.wait().unwrap();

        assert!(is_running(std::process::id()));
        assert!(!is_running(child.id()));
    }

    #[cfg(unix)]
    #[test]
    fn a_wrappers_real_program_is_found_below_it() {
        // The trailing `true` keeps sh from replacing itself with sleep.
        let mut wrapper = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30; true"])
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut found = Vec::new();
        while found.is_empty() && std::time::Instant::now() < deadline {
            found = descendants(wrapper.id(), 0);
        }
        for pid in found.iter().chain([&wrapper.id()]) {
            // SAFETY: plain signal to a test-owned process.
            unsafe { libc::kill(libc::pid_t::try_from(*pid).unwrap(), libc::SIGKILL) };
        }
        let _ = wrapper.wait();

        assert!(!found.is_empty(), "no process found below the wrapper");
    }

    #[test]
    fn a_loop_in_the_parent_links_ends() {
        assert_eq!(below(&[(1, 2), (2, 1)], 1, 0, |_| Some(0)), [2]);
    }

    #[cfg(windows)]
    #[test]
    fn a_shims_real_program_is_found_below_it() {
        let since = u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let mut shim = std::process::Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut found = Vec::new();
        while found.is_empty() && std::time::Instant::now() < deadline {
            found = descendants(shim.id(), since);
        }
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &shim.id().to_string()])
            .output();
        let _ = shim.wait();

        assert!(!found.is_empty(), "no process found below the shim");
    }
}
