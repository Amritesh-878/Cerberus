//! Finds a service's address in its output: a tunnel's public URL, else a local dev server's.

/// A line longer than this is not one a server prints its address on; the rest is dropped.
const MOST_LINE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `cloudflared` also prints the local address it forwards to, so only the public one counts.
    Tunnel,
    Local,
}

impl Rule {
    pub fn for_command(command: &str) -> Self {
        let tunnel = command.split_whitespace().any(|word| {
            let word = word.trim_matches(|c| c == '"' || c == '\'');
            let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
            // npm installs it as `.cmd` and `.ps1` shims on Windows.
            let stem = name.rsplit_once('.').map_or(name, |(stem, ext)| {
                if ["exe", "cmd", "bat", "ps1"]
                    .iter()
                    .any(|e| ext.eq_ignore_ascii_case(e))
                {
                    stem
                } else {
                    name
                }
            });
            stem.eq_ignore_ascii_case("cloudflared")
        });
        if tunnel { Self::Tunnel } else { Self::Local }
    }
}

/// Reads output as it comes, a chunk at a time, and keeps the first URL of the run.
#[derive(Debug)]
pub struct Finder {
    rule: Rule,
    line: Vec<u8>,
    found: Option<String>,
}

impl Finder {
    pub fn new(rule: Rule) -> Self {
        Self {
            rule,
            line: Vec::new(),
            found: None,
        }
    }

    /// The URL, the first time one shows; later ones of the same run are ignored.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<String> {
        if self.found.is_some() {
            return None;
        }
        for &byte in bytes {
            if byte == b'\n' || byte == b'\r' {
                if let Some(url) = self.end_line() {
                    self.found = Some(url.clone());
                    return Some(url);
                }
            } else if self.line.len() < MOST_LINE {
                self.line.push(byte);
            }
        }
        None
    }

    fn end_line(&mut self) -> Option<String> {
        let text = strip_escapes(&String::from_utf8_lossy(&self.line));
        self.line.clear();
        find(self.rule, &text)
    }
}

/// The first URL in one line of plain text, by the rule.
pub fn find(rule: Rule, text: &str) -> Option<String> {
    match rule {
        Rule::Tunnel => text.match_indices("https://").find_map(|(at, _)| {
            let url = url_at(&text[at..]);
            let host = url
                .trim_start_matches("https://")
                .split(['/', ':'])
                .next()?;
            let name = host.strip_suffix(".trycloudflare.com")?;
            (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
                .then(|| url.trim_end_matches('/').to_owned())
        }),
        Rule::Local => text.match_indices("http://").find_map(|(at, _)| {
            let url = url_at(&text[at..]);
            let rest = url.trim_start_matches("http://");
            let (host, port) = rest.split('/').next()?.split_once(':')?;
            let local = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1";
            (local
                && !port.is_empty()
                && port.len() <= 5
                && port.bytes().all(|b| b.is_ascii_digit()))
            .then(|| format!("http://{host}:{port}/"))
        }),
    }
}

// Up to the first character a URL can't hold, then without closing punctuation a log adds after it.
fn url_at(text: &str) -> &str {
    let end = text
        .find(|c: char| {
            c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '|' | '`')
        })
        .unwrap_or(text.len());
    text[..end].trim_end_matches(['.', ',', ';', ')', ']'])
}

/// Drops terminal escapes (colours, cursor moves, titles), so a coloured URL reads as plain text.
pub fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in @..~.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUNNEL_BOX: &str = "2026-11-09T10:00:00Z INF |  https://fake-words-here.trycloudflare.com                  |\r\n";

    #[test]
    fn a_tunnel_line_gives_its_public_url_and_never_the_local_one() {
        let mut finder = Finder::new(Rule::Tunnel);

        let local = finder.feed(b"INF Requesting new quick Tunnel on trycloudflare.com...\r\nINF origin http://localhost:5173\r\n");
        let public = finder.feed(TUNNEL_BOX.as_bytes());

        assert_eq!(local, None);
        assert_eq!(
            public.as_deref(),
            Some("https://fake-words-here.trycloudflare.com")
        );
    }

    #[test]
    fn local_lines_give_localhost_or_127_with_a_port() {
        assert_eq!(
            find(Rule::Local, "  ➜  Local:   http://localhost:5173/"),
            Some("http://localhost:5173/".into())
        );
        assert_eq!(
            find(
                Rule::Local,
                "INFO: Uvicorn running on http://127.0.0.1:8000 (Press CTRL+C to quit)"
            ),
            Some("http://127.0.0.1:8000/".into())
        );
        assert_eq!(
            find(
                Rule::Local,
                "see http://example.com:80/ and http://localhost/"
            ),
            None
        );
        assert_eq!(find(Rule::Local, "http://localhost:99999999/"), None);
    }

    #[test]
    fn colour_codes_around_a_url_are_dropped() {
        let line = "\x1b[32m  ➜\x1b[39m  \x1b[1mLocal\x1b[22m:   \x1b[36mhttp://localhost:\x1b[1m5173\x1b[22m/\x1b[39m\n";
        let mut finder = Finder::new(Rule::Local);

        assert_eq!(
            finder.feed(line.as_bytes()).as_deref(),
            Some("http://localhost:5173/")
        );
    }

    #[test]
    fn a_title_escape_is_not_text() {
        assert_eq!(
            strip_escapes("\x1b]0;http://localhost:1/\x07ok \x1b]2;t\x1b\\done"),
            "ok done"
        );
    }

    #[test]
    fn a_url_split_across_chunks_is_still_found_and_only_the_first_counts() {
        let mut finder = Finder::new(Rule::Local);

        assert_eq!(finder.feed(b"ready on http://local"), None);
        assert_eq!(
            finder
                .feed(b"host:3000\nalso http://localhost:4000\n")
                .as_deref(),
            Some("http://localhost:3000/")
        );
        assert_eq!(finder.feed(b"http://localhost:5000\n"), None);
    }

    #[test]
    fn a_new_run_finds_a_new_url() {
        let mut first = Finder::new(Rule::Tunnel);
        first.feed(TUNNEL_BOX.as_bytes());
        let mut restarted = Finder::new(Rule::Tunnel);

        let again = restarted.feed(b"| https://other-fake-name.trycloudflare.com |\n");

        assert_eq!(
            again.as_deref(),
            Some("https://other-fake-name.trycloudflare.com")
        );
    }

    #[test]
    fn the_rule_follows_the_program_not_a_word_inside_an_argument() {
        assert_eq!(
            Rule::for_command("cloudflared tunnel --url http://localhost:5173"),
            Rule::Tunnel
        );
        assert_eq!(
            Rule::for_command(r#""C:\Tools\cloudflared.exe" tunnel --url x"#),
            Rule::Tunnel
        );
        assert_eq!(
            Rule::for_command("& 'C:/nodejs/cloudflared.cmd' tunnel --url x"),
            Rule::Tunnel
        );
        assert_eq!(Rule::for_command("cloudflared.yml.bak"), Rule::Local);
        assert_eq!(Rule::for_command("npm run dev"), Rule::Local);
        assert_eq!(Rule::for_command("echo not-cloudflared-here"), Rule::Local);
    }
}
