/// Every process below `root`, children first, from Windows' process list.
#[cfg(windows)]
pub fn descendants(root: u32) -> Vec<u32> {
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
    below(&pairs, root)
}

#[cfg(not(windows))]
pub fn descendants(_root: u32) -> Vec<u32> {
    Vec::new()
}

/// `pairs` are (process, parent); the walk is breadth first, so nearer processes come first.
fn below(pairs: &[(u32, u32)], root: u32) -> Vec<u32> {
    let mut found = Vec::new();
    let mut next = std::collections::VecDeque::from([root]);
    while let Some(parent) = next.pop_front() {
        for &(pid, of) in pairs {
            if of == parent && pid != root && !found.contains(&pid) {
                found.push(pid);
                next.push_back(pid);
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

        assert_eq!(below(&pairs, 10), [20, 21, 30]);
        assert_eq!(below(&pairs, 30), Vec::<u32>::new());
    }

    #[test]
    fn a_loop_in_the_parent_links_ends() {
        assert_eq!(below(&[(1, 2), (2, 1)], 1), [2]);
    }

    #[cfg(windows)]
    #[test]
    fn a_shims_real_program_is_found_below_it() {
        let mut shim = std::process::Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut found = Vec::new();
        while found.is_empty() && std::time::Instant::now() < deadline {
            found = descendants(shim.id());
        }
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &shim.id().to_string()])
            .output();
        let _ = shim.wait();

        assert!(!found.is_empty(), "no process found below the shim");
    }
}
