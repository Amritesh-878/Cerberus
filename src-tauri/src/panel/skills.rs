use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use serde::Serialize;

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
}

/// The project's `.claude/skills` first, then the account's `skills`; a name the project has hides the account's.
pub fn read(folder: Option<&Path>, claude_home: &Path) -> Vec<Skill> {
    let mut skills = folder
        .map(|f| in_dir(&f.join(".claude").join("skills"), Scope::Project))
        .unwrap_or_default();
    for skill in in_dir(&claude_home.join("skills"), Scope::Account) {
        if !skills.iter().any(|s| s.name == skill.name) {
            skills.push(skill);
        }
    }
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

        let found = read(Some(&project), &home);

        let listed: Vec<(&str, Option<&str>, Scope)> = found
            .iter()
            .map(|s| (s.name.as_str(), s.description.as_deref(), s.scope))
            .collect();
        assert_eq!(
            listed,
            [
                ("release-notes", Some("Draft the notes."), Scope::Project),
                ("shared", Some("mine"), Scope::Project),
                ("bare", None, Scope::Account),
                (
                    "writing-voice",
                    Some("Plain words, answer first."),
                    Scope::Account
                ),
            ]
        );
    }

    #[test]
    fn missing_folders_have_no_skills() {
        let dir = TestDir::new();

        assert!(read(Some(&dir.path().join("none")), &dir.path().join("none")).is_empty());
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
