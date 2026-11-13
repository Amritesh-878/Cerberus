//! A tab's chat's written files and background tasks, from its hooks.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use serde::Serialize;

use crate::live::event::{Subagent, Task, TaskKind, Tasks, Wrote};

/// The Outputs card lists the newest of these.
const MOST_OUTPUTS: usize = 50;
/// Finished task ids kept so a late list cannot show one as running again.
const MOST_DONE: usize = 256;
/// Finished tasks the card lists, newest first.
const MOST_RECENT: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// The file's name, for the row.
    pub name: String,
    /// Its folder, relative to the tab's when inside it.
    pub folder: String,
    pub path: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub id: String,
    pub kind: TaskKind,
    /// `shell`, or the subagent's type.
    pub label: String,
    /// Claude Code's short summary of it, in memory only.
    pub description: Option<String>,
    /// Milliseconds since the Unix epoch when Cerberus first saw it; Claude Code gives no start time.
    pub since: u64,
    /// When it was seen to finish, in the same clock; `None` while it runs.
    pub ended: Option<u64>,
}

#[derive(Debug, Default)]
pub struct Work {
    outputs: VecDeque<(String, u64)>,
    running: BTreeMap<String, Running>,
    done: VecDeque<String>,
    recent: VecDeque<Running>,
    finished: u32,
}

impl Work {
    /// A written file goes to the top; one written again moves up rather than listing twice.
    pub fn wrote(&mut self, wrote: &Wrote) {
        self.outputs.retain(|(path, _)| path != &wrote.path);
        self.outputs.push_front((wrote.path.clone(), wrote.at));
        self.outputs.truncate(MOST_OUTPUTS);
    }

    /// The list as a turn ended: new running tasks join, and any that left it finished.
    pub fn listed(&mut self, tasks: &Tasks) {
        let gone: Vec<String> = self
            .running
            .keys()
            .filter(|id| !tasks.tasks.iter().any(|t| &t.id == *id && t.running))
            .cloned()
            .collect();
        for id in gone {
            self.finish(&id, tasks.at);
        }
        for task in tasks.tasks.iter().filter(|t| t.running) {
            self.start(task, tasks.at);
            // A subagent's own start hook has no description; the turn's list fills it in.
            if let (Some(row), Some(description)) =
                (self.running.get_mut(&task.id), &task.description)
            {
                row.description.get_or_insert_with(|| description.clone());
            }
        }
    }

    pub fn subagent(&mut self, event: &Subagent) {
        if event.started {
            let task = Task {
                id: event.id.clone(),
                kind: TaskKind::Subagent,
                running: true,
                agent_type: event.agent_type.clone(),
                description: None,
            };
            self.start(&task, event.at);
        } else {
            self.finish(&event.id, event.at);
        }
    }

    fn start(&mut self, task: &Task, at: u64) {
        if self.running.contains_key(&task.id) || self.done.contains(&task.id) {
            return;
        }
        let label = match task.kind {
            TaskKind::Shell => "shell".to_owned(),
            TaskKind::Subagent => task
                .agent_type
                .clone()
                .unwrap_or_else(|| "subagent".to_owned()),
        };
        self.running.insert(
            task.id.clone(),
            Running {
                id: task.id.clone(),
                kind: task.kind,
                label,
                description: task.description.clone(),
                since: at,
                ended: None,
            },
        );
    }

    fn finish(&mut self, id: &str, at: u64) {
        if self.done.iter().any(|d| d == id) {
            return;
        }
        if let Some(mut row) = self.running.remove(id) {
            row.ended = Some(at.max(row.since));
            self.recent.push_front(row);
            self.recent.truncate(MOST_RECENT);
        }
        self.finished = self.finished.saturating_add(1);
        self.done.push_back(id.to_owned());
        if self.done.len() > MOST_DONE {
            self.done.pop_front();
        }
    }

    /// The chat ended at `at`, so whatever was running ended with it.
    pub fn ended(&mut self, at: u64) {
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for id in ids {
            self.finish(&id, at);
        }
    }

    /// Finished tasks, newest first.
    pub fn recent(&self) -> Vec<Running> {
        self.recent.iter().cloned().collect()
    }

    pub fn outputs(&self, tab_folder: &Path) -> Vec<Output> {
        self.outputs
            .iter()
            .map(|(path, at)| shown(path, *at, tab_folder))
            .collect()
    }

    /// Oldest first, as they started.
    pub fn running(&self) -> Vec<Running> {
        let mut all: Vec<Running> = self.running.values().cloned().collect();
        all.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.id.cmp(&b.id)));
        all
    }

    pub fn finished(&self) -> u32 {
        self.finished
    }

    pub fn has_output(&self, path: &str) -> bool {
        self.outputs.iter().any(|(p, _)| p == path)
    }
}

fn shown(path: &str, at: u64, tab_folder: &Path) -> Output {
    let full = Path::new(path);
    let name = full
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
    let parent = full.parent().unwrap_or(full);
    let folder = match parent.strip_prefix(tab_folder) {
        Ok(inside) if inside.as_os_str().is_empty() => String::new(),
        Ok(inside) => inside.to_string_lossy().into_owned(),
        Err(_) => parent.to_string_lossy().into_owned(),
    };
    Output {
        name,
        folder,
        path: path.to_owned(),
        at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::native;

    fn wrote(path: &str, at: u64) -> Wrote {
        Wrote {
            at,
            tool: "Write".into(),
            path: native(path).to_string_lossy().into_owned(),
        }
    }

    fn task(id: &str, kind: TaskKind, running: bool) -> Task {
        Task {
            id: id.into(),
            kind,
            running,
            agent_type: (kind == TaskKind::Subagent).then(|| "general-purpose".into()),
            description: Some(format!("about {id}")),
        }
    }

    fn listed(at: u64, tasks: Vec<Task>) -> Tasks {
        Tasks { at, tasks }
    }

    #[test]
    fn outputs_come_newest_first_once_each_with_folders_inside_the_tab_shortened() {
        let mut work = Work::default();
        work.wrote(&wrote(r"C:\work\src\app.ts", 1));
        work.wrote(&wrote(r"C:\work\readme.md", 2));
        work.wrote(&wrote(r"C:\elsewhere\notes.txt", 3));
        work.wrote(&wrote(r"C:\work\src\app.ts", 4));

        let shown = work.outputs(&native(r"C:\work"));

        let rows: Vec<(&str, String)> = shown
            .iter()
            .map(|o| (o.name.as_str(), o.folder.replace('\\', "/")))
            .collect();
        assert_eq!(
            rows,
            [
                ("app.ts", "src".to_owned()),
                (
                    "notes.txt",
                    native(r"C:\elsewhere").to_string_lossy().replace('\\', "/")
                ),
                ("readme.md", String::new()),
            ]
        );
        assert!(work.has_output(&native(r"C:\work\readme.md").to_string_lossy()));
        assert!(!work.has_output(&native(r"C:\work\other.md").to_string_lossy()));
    }

    #[test]
    fn only_the_newest_fifty_outputs_are_kept() {
        let mut work = Work::default();
        for i in 0..60 {
            work.wrote(&wrote(&format!(r"C:\work\f{i}.txt"), i));
        }

        let shown = work.outputs(&native(r"C:\work"));

        assert_eq!(shown.len(), MOST_OUTPUTS);
        assert_eq!(shown[0].name, "f59.txt");
    }

    #[test]
    fn a_task_runs_from_when_first_seen_until_it_leaves_the_list() {
        let mut work = Work::default();
        work.listed(&listed(10, vec![task("b1", TaskKind::Shell, true)]));
        work.listed(&listed(
            20,
            vec![
                task("b1", TaskKind::Shell, true),
                task("b2", TaskKind::Shell, true),
            ],
        ));

        let running = work.running();
        assert_eq!(running.len(), 2);
        assert_eq!((running[0].id.as_str(), running[0].since), ("b1", 10));
        assert_eq!(running[0].label, "shell");

        work.listed(&listed(30, vec![task("b2", TaskKind::Shell, false)]));
        assert!(work.running().is_empty());
        assert_eq!(work.finished(), 2);
        let recent = work.recent();
        let done: Vec<(&str, Option<&str>, u64, Option<u64>)> = recent
            .iter()
            .map(|r| (r.id.as_str(), r.description.as_deref(), r.since, r.ended))
            .collect();
        assert_eq!(
            done,
            [
                ("b2", Some("about b2"), 20, Some(30)),
                ("b1", Some("about b1"), 10, Some(30)),
            ]
        );
    }

    #[test]
    fn only_the_last_twenty_finished_tasks_are_listed_and_a_subagent_gets_its_description_later() {
        let mut work = Work::default();
        for i in 0..25 {
            let id = format!("s{i}");
            work.listed(&listed(i, vec![task(&id, TaskKind::Shell, true)]));
            work.listed(&listed(i, vec![]));
        }
        assert_eq!(work.recent().len(), MOST_RECENT);
        assert_eq!(work.recent()[0].id, "s24");

        work.subagent(&Subagent {
            at: 100,
            id: "a9".into(),
            agent_type: None,
            started: true,
        });
        assert_eq!(work.running()[0].description, None);
        work.listed(&listed(101, vec![task("a9", TaskKind::Subagent, true)]));
        assert_eq!(work.running()[0].description.as_deref(), Some("about a9"));
        work.ended(150);
        assert_eq!(work.recent()[0].ended, Some(150));
    }

    #[test]
    fn a_subagent_starts_and_stops_by_its_own_hooks_and_a_late_list_cannot_revive_it() {
        let mut work = Work::default();
        let agent = |started, at| Subagent {
            at,
            id: "a1".into(),
            agent_type: Some("general-purpose".into()),
            started,
        };

        work.subagent(&agent(true, 5));
        assert_eq!(work.running()[0].label, "general-purpose");
        work.subagent(&agent(false, 6));
        // Claude Code's SubagentStop can still list it as running.
        work.listed(&listed(7, vec![task("a1", TaskKind::Subagent, true)]));
        work.subagent(&agent(false, 8));

        assert!(work.running().is_empty());
        assert_eq!(work.finished(), 1);
    }
}
