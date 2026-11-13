/// What the system says about processes, as plain functions so tests can stand in for it.
pub struct Probes {
    pub below: fn(u32, u64) -> Vec<u32>,
    pub is_running: fn(u32) -> bool,
    /// Ends a process, but only one started at or after the given Unix milliseconds.
    pub stop: fn(u32, u64),
}

impl Probes {
    pub const SYSTEM: Self = Self {
        below: descendants,
        is_running,
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

/// Unix moves an orphan to launchd, so a parent id is never stale and no start time is needed.
#[cfg(unix)]
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
#[cfg(any(unix, test))]
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
#[cfg(any(unix, test))]
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
