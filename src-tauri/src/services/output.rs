//! A service's recent output, in memory only, for Show output to replay.

use std::collections::VecDeque;

/// About the last 5,000 lines.
pub const MOST_LINES: usize = 5_000;
/// A program that prints without newlines still can't grow it past this.
const MOST_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct Output {
    chunks: VecDeque<Vec<u8>>,
    bytes: usize,
    lines: usize,
}

impl Output {
    pub fn push(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        self.bytes += chunk.len();
        self.lines += newlines(chunk);
        self.chunks.push_back(chunk.to_vec());
        self.trim();
    }

    /// Everything kept, oldest first.
    pub fn replay(&self) -> Vec<u8> {
        let mut all = Vec::with_capacity(self.bytes);
        for chunk in &self.chunks {
            all.extend_from_slice(chunk);
        }
        all
    }

    fn trim(&mut self) {
        while self.lines > MOST_LINES || self.bytes > MOST_BYTES {
            let Some(front) = self.chunks.front_mut() else {
                return;
            };
            // Drop up to and including the first newline, or the whole chunk if it has none.
            let cut = front
                .iter()
                .position(|&b| b == b'\n')
                .map_or(front.len(), |at| at + 1);
            self.bytes -= cut;
            self.lines -= newlines(&front[..cut]);
            if cut == front.len() {
                self.chunks.pop_front();
            } else {
                front.drain(..cut);
            }
        }
    }
}

fn newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == b'\n').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_output_replays_as_it_came() {
        let mut out = Output::default();
        out.push(b"one\r\ntw");
        out.push(b"o\r\n");

        assert_eq!(out.replay(), b"one\r\ntwo\r\n");
    }

    #[test]
    fn only_the_last_lines_are_kept_and_replay_starts_on_a_line() {
        let mut out = Output::default();
        let text: String = (0..MOST_LINES + 10)
            .map(|i| format!("line {i}\n"))
            .collect();
        for chunk in text.as_bytes().chunks(777) {
            out.push(chunk);
        }

        let kept = String::from_utf8(out.replay()).unwrap();

        assert_eq!(kept.lines().count(), MOST_LINES);
        assert!(kept.starts_with("line 10\n"), "{}", &kept[..20]);
        assert!(kept.ends_with(&format!("line {}\n", MOST_LINES + 9)));
    }

    #[test]
    fn output_without_newlines_is_held_to_its_byte_cap() {
        let mut out = Output::default();
        for _ in 0..40 {
            out.push(&[b'x'; 100_000]);
        }

        assert!(out.replay().len() <= MOST_BYTES);
    }
}
