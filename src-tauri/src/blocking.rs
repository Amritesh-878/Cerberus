//! Slow command bodies run on the blocking pool, so the few async workers that answer the webview never wait on a process, a disk or a pipe.

/// Runs `work` on Tauri's blocking pool; a panic in it carries on in the caller, as it would have inline.
pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(value) => value,
        Err(tauri::Error::JoinError(e)) if e.is_panic() => {
            std::panic::resume_unwind(e.into_panic())
        }
        // Cancelled only while the runtime shuts down, when nobody is left waiting for the answer.
        Err(_) => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;

    static HELD: AtomicBool = AtomicBool::new(true);

    #[test]
    fn the_async_workers_still_answer_while_more_bodies_than_workers_block() {
        let workers = std::thread::available_parallelism().map_or(8, usize::from);
        for _ in 0..workers + 2 {
            tauri::async_runtime::spawn(blocking(|| {
                let deadline = Instant::now() + Duration::from_secs(10);
                while HELD.load(Ordering::SeqCst) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }));
        }
        let (tx, rx) = mpsc::channel();
        tauri::async_runtime::spawn(async move { tx.send(()) });

        let answered = rx.recv_timeout(Duration::from_secs(3));
        HELD.store(false, Ordering::SeqCst);

        assert!(
            answered.is_ok(),
            "a quick command waited behind blocked ones"
        );
    }

    /// Commands that only touch memory, so they may run inline on a worker; any other plain `fn` command would block one.
    const INLINE: [&str; 7] = [
        "bridge_discard",
        "bridge_snapshot",
        "chat_find_close",
        "open_take",
        "pty_ack",
        "service_unwatch",
        "services_view",
    ];

    fn plain_commands(dir: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                plain_commands(&path, found);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            let mut after_attribute = false;
            for line in source.lines().map(str::trim) {
                if line.starts_with("#[tauri::command") {
                    after_attribute = true;
                } else if after_attribute && line.starts_with("pub fn ") {
                    let name = &line["pub fn ".len()..];
                    found.push(name[..name.find('(').unwrap()].to_owned());
                    after_attribute = false;
                } else if !line.starts_with("///") && !line.starts_with("#[") {
                    after_attribute = false;
                }
            }
        }
    }

    #[test]
    fn only_memory_only_commands_run_inline() {
        let mut found = Vec::new();
        plain_commands(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut found,
        );
        found.sort();

        assert_eq!(found, INLINE);
    }
}
