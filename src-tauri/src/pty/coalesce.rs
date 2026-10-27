use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

pub struct Batching {
    /// Output arriving within this long of the last emit is streaming, and waits this long for more.
    pub window: Duration,
    pub max: usize,
}

/// Forwards chunks from `rx` to `emit`, merging queued chunks and, while output streams, what follows.
pub fn pump(rx: &Receiver<Vec<u8>>, batching: &Batching, mut emit: impl FnMut(Vec<u8>)) {
    let mut last_emit: Option<Instant> = None;
    while let Ok(mut batch) = rx.recv() {
        while batch.len() < batching.max {
            match rx.try_recv() {
                Ok(more) => batch.extend_from_slice(&more),
                Err(_) => break,
            }
        }
        // Waiting costs echo latency, and Windows rounds short waits up to ~15 ms, so only streams wait.
        if last_emit.is_some_and(|at| at.elapsed() < batching.window) {
            let deadline = Instant::now() + batching.window;
            while batch.len() < batching.max {
                match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(more) => batch.extend_from_slice(&more),
                    Err(_) => break,
                }
            }
        }
        emit(batch);
        last_emit = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    const PAUSE: Duration = Duration::from_millis(100);

    fn run_queued(chunks: &[&[u8]], batching: &Batching) -> Vec<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        for chunk in chunks {
            tx.send(chunk.to_vec()).unwrap();
        }
        drop(tx);
        let mut out = Vec::new();
        pump(&rx, batching, |batch| out.push(batch));
        out
    }

    /// Sends each chunk after a pause and returns the batches emitted.
    fn run_paused(chunks: &[&[u8]], batching: &Batching) -> Vec<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        let chunks: Vec<Vec<u8>> = chunks.iter().map(|c| c.to_vec()).collect();
        let sender = thread::spawn(move || {
            for chunk in chunks {
                thread::sleep(PAUSE);
                tx.send(chunk).unwrap();
            }
        });
        let mut out = Vec::new();
        pump(&rx, batching, |batch| out.push(batch));
        sender.join().unwrap();
        out
    }

    #[test]
    fn queued_chunks_merge_into_one_emit() {
        let batching = Batching {
            window: Duration::from_millis(5),
            max: 1024,
        };

        let out = run_queued(&[b"ab", b"cd", b"ef"], &batching);

        assert_eq!(out, [b"abcdef".to_vec()]);
    }

    #[test]
    fn a_full_batch_is_emitted_without_waiting() {
        let batching = Batching {
            window: Duration::from_secs(60),
            max: 4,
        };

        let out = run_queued(&[b"abcd", b"ef"], &batching);

        assert_eq!(out, [b"abcd".to_vec(), b"ef".to_vec()]);
    }

    #[test]
    fn output_after_a_quiet_spell_goes_out_at_once() {
        let batching = Batching {
            window: Duration::from_millis(5),
            max: 1024,
        };

        let out = run_paused(&[b"ab", b"cd"], &batching);

        assert_eq!(out, [b"ab".to_vec(), b"cd".to_vec()]);
    }

    #[test]
    fn streaming_output_waits_for_more_within_the_window() {
        let batching = Batching {
            window: Duration::from_secs(60),
            max: 1024,
        };

        let out = run_paused(&[b"ab", b"cd", b"ef"], &batching);

        assert_eq!(out, [b"ab".to_vec(), b"cdef".to_vec()]);
    }
}
