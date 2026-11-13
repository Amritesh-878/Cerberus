//! A `.cmd` or `.bat` program, such as npm's `claude.cmd`, runs inside `cmd`, which reads `&`, `|`, `<`, `>` and `^` in its arguments as syntax.

use std::ffi::OsString;
use std::path::Path;

use portable_pty::CommandBuilder;

/// `cmd` reads the line it runs from here: portable-pty quotes arguments by C rules, which `cmd` does not follow.
const LINE_VAR: &str = "CERBERUS_CMD_LINE";

/// `cmd` running `program` with `args`; `None` when an argument can't be passed through `cmd` safely.
pub fn command(program: &Path, args: &[String]) -> Option<CommandBuilder> {
    let line = line(program, args)?;
    let mut command =
        CommandBuilder::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()));
    // `cmd` expands the variable once, so a `%` inside the line it holds is not expanded again.
    command.args(["/d", "/v:off", "/c", &format!("%{LINE_VAR}%")]);
    command.env(LINE_VAR, line);
    Some(command)
}

/// Whether Windows would start `program` through `cmd`.
pub fn is_batch(program: &Path) -> bool {
    program
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
}

/// The line for `cmd` to run, every part quoted so its syntax characters stay text; `None` when a part holds a quote or a line break, which no `cmd` quoting can carry.
fn line(program: &Path, args: &[String]) -> Option<OsString> {
    let program = program.to_str()?;
    let mut line = String::new();
    for part in std::iter::once(program).chain(args.iter().map(String::as_str)) {
        if part.contains(['"', '\r', '\n', '\0']) {
            return None;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push('"');
        line.push_str(part);
        // Doubled, so the program's C-rule parser does not read the closing quote as escaped.
        let trailing = part.len() - part.trim_end_matches('\\').len();
        line.push_str(&"\\".repeat(trailing));
        line.push('"');
    }
    Some(line.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_of(args: &[&str]) -> Option<String> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        line(Path::new(r"C:\npm\claude.cmd"), &args).map(|l| l.into_string().unwrap())
    }

    #[test]
    fn only_cmd_and_bat_files_go_through_cmd() {
        assert!(is_batch(Path::new(r"C:\npm\claude.cmd")));
        assert!(is_batch(Path::new(r"C:\tools\RUN.BAT")));
        assert!(!is_batch(Path::new(r"C:\bin\claude.exe")));
        assert!(!is_batch(Path::new("/usr/local/bin/claude")));
    }

    #[test]
    fn every_part_is_quoted_and_syntax_characters_stay_inside_the_quotes() {
        assert_eq!(
            line_of(&[r"--add-dir=C:\code\R&D", "100%", "a^b|c", "with space"]).unwrap(),
            r#""C:\npm\claude.cmd" "--add-dir=C:\code\R&D" "100%" "a^b|c" "with space""#
        );
    }

    #[test]
    fn a_trailing_backslash_is_doubled_and_an_empty_argument_kept() {
        assert_eq!(
            line_of(&[r"--add-dir=C:\code\", ""]).unwrap(),
            r#""C:\npm\claude.cmd" "--add-dir=C:\code\\" """#
        );
    }

    #[test]
    fn a_quote_or_line_break_is_refused() {
        assert_eq!(line_of(&[r#"--add-dir=C:\a"&calc"#]), None);
        assert_eq!(line_of(&["one\r\ntwo"]), None);
    }
}
