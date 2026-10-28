use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub name: String,
    pub config_dir: PathBuf,
}

// Files Claude Code writes into a config dir; credentials are deliberately not among them.
const MARKERS: [&str; 3] = [".claude.json", "settings.json", "projects"];

pub fn candidates(home: &Path) -> Vec<Candidate> {
    let mut found = Vec::new();
    let default = home.join(".claude");
    // The default account keeps its `.claude.json` in the home folder, beside the dir.
    if default.is_dir() && (home.join(".claude.json").is_file() || used_by_claude(&default)) {
        found.push(Candidate {
            name: "default".to_owned(),
            config_dir: default,
        });
    }
    let Ok(entries) = fs::read_dir(home) else {
        return found;
    };
    let mut others: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let name = file_name.to_str()?.strip_prefix(".claude-")?;
            let dir = entry.path();
            (!name.is_empty() && dir.is_dir() && used_by_claude(&dir)).then(|| Candidate {
                name: name.to_owned(),
                config_dir: dir,
            })
        })
        .collect();
    others.sort_by(|a, b| a.name.cmp(&b.name));
    found.extend(others);
    found
}

fn used_by_claude(dir: &Path) -> bool {
    MARKERS.iter().any(|marker| dir.join(marker).exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    fn names(home: &Path) -> Vec<String> {
        candidates(home).into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn finds_the_default_and_named_config_dirs_in_order() {
        let home = TestDir::new();
        let h = home.path();
        fs::create_dir_all(h.join(".claude")).unwrap();
        fs::write(h.join(".claude.json"), "{}").unwrap();
        fs::create_dir_all(h.join(".claude-work").join("projects")).unwrap();
        fs::create_dir_all(h.join(".claude-acc2")).unwrap();
        fs::write(h.join(".claude-acc2").join("settings.json"), "{}").unwrap();

        let found = candidates(h);

        assert_eq!(
            found.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["default", "acc2", "work"]
        );
        assert_eq!(found[1].config_dir, h.join(".claude-acc2"));
    }

    #[test]
    fn the_default_dir_counts_with_its_own_claude_json() {
        let home = TestDir::new();
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::write(home.path().join(".claude").join(".claude.json"), "{}").unwrap();

        assert_eq!(names(home.path()), ["default"]);
    }

    #[test]
    fn skips_folders_claude_never_used_and_look_alike_files() {
        let home = TestDir::new();
        let h = home.path();
        fs::create_dir_all(h.join(".claude")).unwrap();
        fs::create_dir_all(h.join(".claude-empty")).unwrap();
        fs::create_dir_all(h.join(".claude-").join("projects")).unwrap();
        fs::create_dir_all(h.join("claude-x").join("projects")).unwrap();
        fs::write(h.join(".claude.json.backup"), "{}").unwrap();
        fs::write(h.join(".claude-notes"), "{}").unwrap();

        assert!(names(h).is_empty());
    }

    #[test]
    fn a_missing_home_finds_nothing() {
        let home = TestDir::new();

        assert!(names(&home.path().join("gone")).is_empty());
    }
}
