//! Whether a message written to a tab's inbox showed up there, and the fallback that types it in.

/// What Claude Code prints when a peer message arrives, with spaces removed.
const ARRIVED: &str = "Messagefrom@";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Parse {
    Text,
    Escape,
    /// Inside `ESC [`, until a final byte.
    Csi,
    /// Inside `ESC ]` or another string, until BEL or `ESC \`.
    Str,
    StrEscape,
}

/// Reads a tab's terminal output for the line a peer message prints, across chunk boundaries.
pub struct Sighting {
    parse: Parse,
    tail: String,
}

impl Default for Sighting {
    fn default() -> Self {
        Self {
            parse: Parse::Text,
            tail: String::new(),
        }
    }
}

impl Sighting {
    /// True once the arrival line shows; whitespace is dropped, since the terminal may move the cursor instead of printing a space.
    pub fn feed(&mut self, bytes: &[u8]) -> bool {
        for &byte in bytes {
            self.parse = match (self.parse, byte) {
                (Parse::Text, 0x1b) => Parse::Escape,
                (Parse::Text, b) => {
                    if b.is_ascii_graphic() {
                        self.tail.push(char::from(b));
                    }
                    Parse::Text
                }
                (Parse::Escape, b'[') => Parse::Csi,
                (Parse::Escape, b']' | b'P' | b'_' | b'^') => Parse::Str,
                (Parse::Escape, _) => Parse::Text,
                (Parse::Csi, 0x40..=0x7e) => Parse::Text,
                (Parse::Csi, _) => Parse::Csi,
                (Parse::Str, 0x07) => Parse::Text,
                (Parse::Str, 0x1b) => Parse::StrEscape,
                (Parse::Str, _) => Parse::Str,
                (Parse::StrEscape, b'\\') => Parse::Text,
                (Parse::StrEscape, _) => Parse::Str,
            };
            if self.tail.ends_with(ARRIVED) {
                return true;
            }
            if self.tail.len() > 4 * ARRIVED.len() {
                self.tail.drain(..self.tail.len() - ARRIVED.len());
            }
        }
        false
    }
}

/// The text as a bracketed paste with no Enter; control characters go, so it cannot end the paste and submit itself.
pub fn prefill(text: &str) -> String {
    let lines = text.replace("\r\n", "\n").replace('\r', "\n");
    let clean: String = lines
        .chars()
        .filter(|c| matches!(c, '\n' | '\t') || !c.is_control())
        .collect();
    format!("\x1b[200~{clean}\x1b[201~")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrival_line_is_seen_through_escapes_and_split_chunks() {
        let mut sighting = Sighting::default();

        assert!(!sighting.feed(b"\x1b[38;2;1;2;3m\xe2\x80\xba Message\x1b[1Cfr"));
        assert!(!sighting.feed(b"\x1b]0;title\x07om "));
        assert!(sighting.feed(b"@work-planner: hi"));
    }

    #[test]
    fn other_output_is_not_an_arrival() {
        let mut sighting = Sighting::default();

        assert!(!sighting.feed(b"A message from the build: ok\r\n"));
        assert!(!sighting.feed("Message from you".as_bytes()));
    }

    #[test]
    fn prefill_never_presses_enter() {
        let typed = prefill("one\r\ntwo\rthree\x1b[201~\r\x03four\u{9b}");

        assert_eq!(typed, "\x1b[200~one\ntwo\nthree[201~\nfour\x1b[201~");
        assert!(!typed.contains('\r'));
    }
}
