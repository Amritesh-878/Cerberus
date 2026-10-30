pub mod launch;
mod pipe;
mod shell;

use std::{fmt, io};

use serde::Serialize;

pub use launch::{Pending, from_command_line, listen, on_second_launch, request};
pub use shell::console;

use crate::saved::{ProjectId, SavedProject, SavedTab, SavedTree, TabId, TreeError};

pub const USAGE: &str = "usage: cerberus [open <project>[/<tab>]]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub project: String,
    pub tab: Option<String>,
}

impl fmt::Display for Spec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.tab {
            Some(tab) => write!(f, "{}/{tab}", self.project),
            None => f.write_str(&self.project),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Start,
    Open(Spec),
}

/// Words after `open` are joined, so an unquoted name with spaces still reads as one.
pub fn parse(args: &[String]) -> Option<Command> {
    let Some((verb, rest)) = args.split_first() else {
        return Some(Command::Start);
    };
    if verb != "open" {
        return None;
    }
    let joined = rest.join(" ");
    let (project, tab) = match joined.split_once('/') {
        Some((project, tab)) => (project, Some(tab)),
        None => (joined.as_str(), None),
    };
    let project = project.trim();
    if project.is_empty() {
        return None;
    }
    Some(Command::Open(Spec {
        project: project.to_owned(),
        tab: tab
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned),
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub project_id: ProjectId,
    pub tab_id: Option<TabId>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Found<'a> {
    pub project: &'a SavedProject,
    pub tab: Option<&'a SavedTab>,
}

impl Found<'_> {
    pub fn target(&self) -> Target {
        Target {
            project_id: self.project.id,
            tab_id: self.tab.map(|t| t.id),
        }
    }
}

impl fmt::Display for Found<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.tab {
            Some(tab) => write!(f, "{} / {}", self.project.name, tab.title),
            None => write!(f, "{}", self.project.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum What {
    Project,
    TabIn(String),
}

impl fmt::Display for What {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project => f.write_str("project"),
            Self::TabIn(project) => write!(f, "tab in {project}"),
        }
    }
}

/// Names only, never chat content, since the command line prints them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Miss {
    Nothing {
        what: What,
        query: String,
        close: Vec<String>,
        known: Vec<String>,
    },
    Several {
        what: What,
        query: String,
        matches: Vec<String>,
    },
}

fn list(f: &mut fmt::Formatter<'_>, names: &[String]) -> fmt::Result {
    names.iter().try_for_each(|name| write!(f, "\n  {name}"))
}

impl fmt::Display for Miss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nothing {
                what,
                query,
                close,
                known,
            } => {
                write!(f, "No {what} matches \"{query}\".")?;
                if !close.is_empty() {
                    f.write_str(" Close matches:")?;
                    list(f, close)
                } else if !known.is_empty() {
                    f.write_str(" Known:")?;
                    list(f, known)
                } else {
                    f.write_str(" There are none saved yet.")
                }
            }
            Self::Several {
                what,
                query,
                matches,
            } => {
                write!(f, "\"{query}\" matches more than one {what}:")?;
                list(f, matches)?;
                f.write_str("\nRename one to tell them apart.")
            }
        }
    }
}

fn same_text(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn folder_name(folder: &str) -> &str {
    let trimmed = folder.trim_end_matches(['/', '\\']);
    trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != cb))
                .min(row[j] + 1)
                .min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

fn is_close(query: &str, name: &str) -> bool {
    let (query, name) = (query.to_lowercase(), name.to_lowercase());
    name.contains(&query) || edit_distance(&query, &name) <= query.chars().count().div_ceil(3)
}

/// The first rule that matches anything wins, so an exact name beats a looser one.
fn first_tier<'a, T>(items: &[&'a T], rules: &[&dyn Fn(&T) -> bool]) -> Vec<&'a T> {
    rules
        .iter()
        .map(|rule| items.iter().copied().filter(|&item| rule(item)).collect())
        .find(|hits: &Vec<&T>| !hits.is_empty())
        .unwrap_or_default()
}

fn dedup(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = Vec::new();
    for name in names {
        if !seen.contains(&name) {
            seen.push(name);
        }
    }
    seen
}

fn find_project<'a>(tree: &'a SavedTree, query: &str) -> Result<&'a SavedProject, Miss> {
    let projects: Vec<&SavedProject> = tree.projects.iter().collect();
    let hits = first_tier(
        &projects,
        &[
            &|p: &SavedProject| p.name == query,
            &|p: &SavedProject| same_text(&p.name, query),
            &|p: &SavedProject| same_text(folder_name(&p.folder), query),
        ],
    );
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(Miss::Nothing {
            what: What::Project,
            query: query.to_owned(),
            close: dedup(
                projects
                    .iter()
                    .filter(|p| is_close(query, &p.name) || is_close(query, folder_name(&p.folder)))
                    .map(|p| p.name.clone()),
            ),
            known: dedup(projects.iter().map(|p| p.name.clone())),
        }),
        several => Err(Miss::Several {
            what: What::Project,
            query: query.to_owned(),
            matches: several
                .iter()
                .map(|p| format!("{} ({})", p.name, p.folder))
                .collect(),
        }),
    }
}

fn find_tab<'a>(
    tree: &'a SavedTree,
    project: &SavedProject,
    query: &str,
) -> Result<&'a SavedTab, Miss> {
    let tabs: Vec<&SavedTab> = tree
        .tabs
        .iter()
        .filter(|t| t.project_id == project.id)
        .collect();
    let hits = first_tier(
        &tabs,
        &[&|t: &SavedTab| t.title == query, &|t: &SavedTab| {
            same_text(&t.title, query)
        }],
    );
    let what = || What::TabIn(project.name.clone());
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(Miss::Nothing {
            what: what(),
            query: query.to_owned(),
            close: dedup(
                tabs.iter()
                    .filter(|t| is_close(query, &t.title))
                    .map(|t| t.title.clone()),
            ),
            known: dedup(tabs.iter().map(|t| t.title.clone())),
        }),
        several => Err(Miss::Several {
            what: what(),
            query: query.to_owned(),
            matches: several.iter().map(|t| t.title.clone()).collect(),
        }),
    }
}

/// Matches the project by name, then case-insensitively, then by folder name; the tab by title the same way.
pub fn resolve<'a>(tree: &'a SavedTree, spec: &Spec) -> Result<Found<'a>, Miss> {
    let project = find_project(tree, &spec.project)?;
    let tab = spec
        .tab
        .as_deref()
        .map(|title| find_tab(tree, project, title))
        .transpose()?;
    Ok(Found { project, tab })
}

/// What the command line does before any window: run the app, or print and exit with a code.
#[derive(Debug, PartialEq, Eq)]
pub enum Checked {
    Run(Command, Option<String>),
    Exit(i32, String),
}

pub const MISSED: i32 = 1;
pub const BAD_USAGE: i32 = 2;

pub fn opening(found: &Found<'_>) -> String {
    format!("Opening {found}")
}

/// A running app answers from its live tree; only without one is the saved file read.
pub fn check(
    args: &[String],
    ask: impl FnOnce(&[String]) -> io::Result<Option<(i32, String)>>,
    read: impl FnOnce() -> Result<SavedTree, TreeError>,
) -> Checked {
    let Some(command) = parse(args) else {
        return Checked::Exit(BAD_USAGE, USAGE.to_owned());
    };
    let Command::Open(spec) = &command else {
        return Checked::Run(command, None);
    };
    match ask(args) {
        Ok(Some((code, message))) => return Checked::Exit(code, message),
        Ok(None) => {}
        Err(e) => {
            return Checked::Exit(MISSED, format!("could not reach the running Cerberus: {e}"));
        }
    }
    let tree = match read() {
        Ok(tree) => tree,
        Err(e) => return Checked::Exit(MISSED, e.to_string()),
    };
    let note = match resolve(&tree, spec) {
        Ok(found) => opening(&found),
        Err(miss) => return Checked::Exit(MISSED, miss.to_string()),
    };
    Checked::Run(command, Some(note))
}

#[cfg(test)]
mod tests;
