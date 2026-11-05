use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

// A skill's front matter sits at the top; the rest of the file is never read.
const HEAD_BYTES: u64 = 16 * 1024;
const MAX_SKILLS: usize = 200;
const MAX_DESCRIPTION: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Project,
    Account,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    pub scope: Scope,
    /// How often the account has used it, from Claude Code's own count.
    pub uses: u64,
}

// Only each skill's name and count are deserialized; every other key in the file is skipped unparsed.
#[derive(Deserialize)]
struct Usage {
    #[serde(default, rename = "usageCount")]
    count: u64,
}

#[derive(Deserialize)]
struct UsageConfig {
    #[serde(default, rename = "skillUsage")]
    usage: BTreeMap<String, Usage>,
}

/// Each skill's use count from an account's `.claude.json`; empty when the file is missing or unreadable.
pub fn usage(account_config: &Path) -> BTreeMap<String, u64> {
    fs::read(account_config)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UsageConfig>(&bytes).ok())
        .map(|config| {
            config
                .usage
                .into_iter()
                .map(|(name, usage)| (name, usage.count))
                .collect()
        })
        .unwrap_or_default()
}

/// The project's `.claude/skills` and the account's `skills`, most used first; a name the project has hides the account's.
pub fn read(
    folder: Option<&Path>,
    claude_home: &Path,
    usage: &BTreeMap<String, u64>,
) -> Vec<Skill> {
    let mut skills = folder
        .map(|f| in_dir(&f.join(".claude").join("skills"), Scope::Project))
        .unwrap_or_default();
    for skill in in_dir(&claude_home.join("skills"), Scope::Account) {
        if !skills.iter().any(|s| s.name == skill.name) {
            skills.push(skill);
        }
    }
    for skill in &mut skills {
        skill.uses = usage.get(&skill.name).copied().unwrap_or(0);
    }
    skills.sort_by(|a, b| {
        b.uses
            .cmp(&a.uses)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    skills.truncate(MAX_SKILLS);
    skills
}

fn in_dir(dir: &Path, scope: Scope) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut skills: Vec<Skill> = entries
        .flatten()
        .filter_map(|entry| {
            let folder = entry.file_name().to_string_lossy().into_owned();
            let head = head(&entry.path().join("SKILL.md"))?;
            let (name, description) = front_matter(&head);
            Some(Skill {
                name: name.unwrap_or(folder),
                description,
                scope,
                uses: 0,
            })
        })
        .take(MAX_SKILLS)
        .collect();
    skills.sort_by_key(|s| s.name.to_lowercase());
    skills
}

fn head(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(HEAD_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// `name` and `description` from YAML front matter, block scalars folded onto one line.
fn front_matter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return (None, None);
    }
    let block: Vec<&str> = lines.take_while(|l| l.trim_end() != "---").collect();
    let value = |key: &str| {
        let at = block
            .iter()
            .position(|l| l.strip_prefix(key).is_some_and(|r| r.starts_with(':')))?;
        let first = block[at][key.len() + 1..].trim();
        let text = if first.is_empty() || first.starts_with('|') || first.starts_with('>') {
            block[at + 1..]
                .iter()
                .take_while(|l| l.starts_with(' ') || l.starts_with('\t') || l.is_empty())
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            unquoted(first).to_owned()
        };
        Some(text).filter(|t| !t.is_empty())
    };
    let description = value("description").map(|d| {
        let one_line = d.split_whitespace().collect::<Vec<_>>().join(" ");
        match one_line.char_indices().nth(MAX_DESCRIPTION) {
            Some((cut, _)) => format!("{}…", &one_line[..cut]),
            None => one_line,
        }
    });
    (value("name"), description)
}

fn unquoted(value: &str) -> &str {
    ['"', '\'']
        .into_iter()
        .find_map(|q| value.strip_prefix(q)?.strip_suffix(q))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    fn skill(root: &Path, folder: &str, text: &str) {
        let dir = root.join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), text).unwrap();
    }

    #[test]
    fn project_and_account_skills_are_read_with_their_descriptions() {
        let dir = TestDir::new();
        let project = dir.path().join("proj");
        let home = dir.path().join("home");
        let project_skills = project.join(".claude").join("skills");
        skill(
            &project_skills,
            "release",
            "---\nname: release-notes\ndescription: \"Draft the notes.\"\n---\nBody\n",
        );
        skill(
            &project_skills,
            "shared",
            "---\nname: shared\ndescription: mine\n---\n",
        );
        let account_skills = home.join("skills");
        skill(
            &account_skills,
            "voice",
            "---\nname: writing-voice\ndescription: >-\n  Plain words,\n  answer first.\nother: x\n---\n",
        );
        skill(
            &account_skills,
            "shared",
            "---\nname: shared\ndescription: theirs\n---\n",
        );
        skill(&account_skills, "bare", "No front matter here.\n");
        fs::create_dir_all(account_skills.join("empty")).unwrap();

        let found = read(Some(&project), &home, &BTreeMap::new());

        let listed: Vec<(&str, Option<&str>, Scope)> = found
            .iter()
            .map(|s| (s.name.as_str(), s.description.as_deref(), s.scope))
            .collect();
        assert_eq!(
            listed,
            [
                ("bare", None, Scope::Account),
                ("release-notes", Some("Draft the notes."), Scope::Project),
                ("shared", Some("mine"), Scope::Project),
                (
                    "writing-voice",
                    Some("Plain words, answer first."),
                    Scope::Account
                ),
            ]
        );
    }

    fn usage_config(dir: &TestDir, text: &str) -> std::path::PathBuf {
        let config = dir.path().join("acct").join(".claude.json");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, text).unwrap();
        config
    }

    #[test]
    fn skills_are_listed_most_used_first_then_by_name() {
        let dir = TestDir::new();
        let home = dir.path().join("home");
        for name in ["alpha", "Beta", "gamma", "delta"] {
            skill(
                &home.join("skills"),
                name,
                &format!("---\nname: {name}\n---\n"),
            );
        }
        let config = usage_config(
            &dir,
            r#"{
                "oauthAccount": { "emailAddress": "fake-account@example.com" },
                "primaryApiKey": "FAKE-KEY-SECRET",
                "skillUsage": {
                    "gamma": { "usageCount": 7, "lastUsedAt": 1762300000000 },
                    "delta": { "usageCount": 2, "lastUsedAt": 1762200000000 },
                    "removed-skill": { "usageCount": 40, "lastUsedAt": 1762100000000 }
                },
                "projects": { "C:/x": { "history": [{ "display": "FAKE-PROMPT-SECRET" }] } }
            }"#,
        );

        let found = read(None, &home, &usage(&config));

        let listed: Vec<(&str, u64)> = found.iter().map(|s| (s.name.as_str(), s.uses)).collect();
        assert_eq!(
            listed,
            [("gamma", 7), ("delta", 2), ("alpha", 0), ("Beta", 0)]
        );
        let shown = format!("{}{found:?}", serde_json::to_string(&found).unwrap());
        for secret in ["FAKE", "fake-account", "removed-skill", "1762"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
    }

    #[test]
    fn usage_is_empty_without_a_readable_count() {
        let dir = TestDir::new();

        assert!(usage(&dir.path().join("missing.json")).is_empty());
        let broken = usage_config(&dir, r#"{ "skillUsage": { "x": "FAKE-SECRET" "#);
        assert!(usage(&broken).is_empty());
        let none = usage_config(&dir, r#"{ "skillUsage": { "x": { "lastUsedAt": 1 } } }"#);
        assert_eq!(usage(&none), BTreeMap::from([("x".to_owned(), 0)]));
    }

    #[test]
    fn missing_folders_have_no_skills() {
        let dir = TestDir::new();

        assert!(
            read(
                Some(&dir.path().join("none")),
                &dir.path().join("none"),
                &BTreeMap::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn front_matter_without_a_name_or_closing_line_still_reads() {
        assert_eq!(
            front_matter("\u{feff}---\r\ndescription: 'One line.'\r\n"),
            (None, Some("One line.".to_owned()))
        );
        assert_eq!(front_matter("name: x\n"), (None, None));
        let long = format!("---\ndescription: {}\n---\n", "word ".repeat(100));
        let (_, description) = front_matter(&long);
        assert!(description.unwrap().ends_with('…'));
    }
}
