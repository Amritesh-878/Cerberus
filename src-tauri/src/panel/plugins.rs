use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::panel::mcp::{Server, parse, same_folder};

/// An installed plugin that Claude Code would load, named as it prefixes the plugin's skills and servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    pub root: PathBuf,
}

// Only where each install sits and for which folder, with versions, commit hashes and dates skipped.
#[derive(Deserialize)]
struct Install {
    #[serde(rename = "installPath")]
    path: PathBuf,
    scope: Option<String>,
    #[serde(rename = "projectPath")]
    project: Option<String>,
}

#[derive(Deserialize)]
struct Installed {
    #[serde(default)]
    plugins: BTreeMap<String, Vec<Install>>,
}

/// An `enabledPlugins` value of `true`, `false`, or a list of version constraints, which enables it.
#[derive(Deserialize)]
#[serde(untagged)]
enum Flag {
    On(bool),
    Versions(#[expect(dead_code, reason = "only that a list matched is used")] Vec<IgnoredAny>),
    Invalid(IgnoredAny),
}

// Only the plugin switches and the synced-skills switch are read, with env, hooks, permissions and the rest skipped unparsed.
#[derive(Deserialize)]
struct Settings {
    #[serde(default, rename = "enabledPlugins")]
    enabled: BTreeMap<String, Flag>,
    #[serde(rename = "syncClaudeAiSkills")]
    sync_skills: Option<bool>,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(rename = "defaultEnabled")]
    default_enabled: Option<bool>,
}

/// The plugins that load, and the files that exist but could not be read, named without their content.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Enabled {
    pub plugins: Vec<Plugin>,
    pub unreadable: Vec<String>,
}

/// User, project and local settings, in Claude Code's order, so a later file's switch wins.
fn switches(
    claude_home: &Path,
    folder: Option<&Path>,
    unreadable: &mut Vec<String>,
) -> BTreeMap<String, bool> {
    let mut files = vec![(
        claude_home.join("settings.json"),
        "the account's settings.json",
    )];
    if let Some(folder) = folder {
        let project = folder.join(".claude");
        files.push((project.join("settings.json"), ".claude/settings.json"));
        files.push((
            project.join("settings.local.json"),
            ".claude/settings.local.json",
        ));
    }
    let mut on = BTreeMap::new();
    for (file, label) in files {
        let settings = match parse::<Settings>(&file) {
            None => continue,
            Some(Err(())) => {
                unreadable.push(label.to_owned());
                continue;
            }
            Some(Ok(settings)) => settings,
        };
        for (id, flag) in settings.enabled {
            match flag {
                Flag::On(value) => {
                    on.insert(id, value);
                }
                Flag::Versions(_) => {
                    on.insert(id, true);
                }
                Flag::Invalid(_) => {}
            }
        }
    }
    on
}

fn applies(install: &Install, folder: Option<&Path>) -> bool {
    match install.scope.as_deref() {
        Some("project" | "local") => folder
            .zip(install.project.as_deref())
            .is_some_and(|(folder, project)| same_folder(project, folder)),
        _ => true,
    }
}

/// The plugins a `claude` started in `folder` under this config dir loads, by name.
pub fn enabled(claude_home: &Path, folder: Option<&Path>) -> Enabled {
    let mut unreadable = Vec::new();
    let switches = switches(claude_home, folder, &mut unreadable);
    let path = claude_home.join("plugins").join("installed_plugins.json");
    let installed = match parse::<Installed>(&path) {
        None => {
            return Enabled {
                plugins: Vec::new(),
                unreadable,
            };
        }
        Some(Err(())) => {
            unreadable.push("the account's installed_plugins.json".to_owned());
            return Enabled {
                plugins: Vec::new(),
                unreadable,
            };
        }
        Some(Ok(installed)) => installed,
    };
    let mut found: Vec<Plugin> = installed
        .plugins
        .into_iter()
        .filter_map(|(id, installs)| {
            let name = id.split('@').next().filter(|n| !n.is_empty())?.to_owned();
            let install = installs.into_iter().find(|i| applies(i, folder))?;
            let root = install.path;
            let on = switches.get(&id).copied().unwrap_or_else(|| {
                let manifest = root.join(".claude-plugin").join("plugin.json");
                parse::<Manifest>(&manifest)
                    .and_then(Result::ok)
                    .and_then(|m| m.default_enabled)
                    .unwrap_or(true)
            });
            (on && root.is_dir()).then_some(Plugin { name, root })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found.dedup_by(|a, b| a.name == b.name);
    Enabled {
        plugins: found,
        unreadable,
    }
}

/// Whether claude.ai's synced skills load, which the user's settings can turn off, a broken file being named in Connectors.
pub fn synced_skills_on(claude_home: &Path) -> bool {
    parse::<Settings>(&claude_home.join("settings.json"))
        .and_then(Result::ok)
        .and_then(|s| s.sync_skills)
        .unwrap_or(true)
}

// A plugin's server file may wrap its servers in `mcpServers` or list them at the top.
#[derive(Deserialize)]
struct MaybeWrapped {
    #[serde(rename = "mcpServers")]
    servers: Option<BTreeMap<String, Server>>,
}

fn plugin_file(path: &Path) -> Option<Result<BTreeMap<String, Server>, ()>> {
    Some(match parse::<MaybeWrapped>(path)? {
        Ok(MaybeWrapped {
            servers: Some(servers),
        }) => Ok(servers),
        Ok(MaybeWrapped { servers: None }) => parse(path)?,
        Err(()) => Err(()),
    })
}

// A manifest's `mcpServers` is a map of servers, a file path or a list, each tried alone so none is buffered.
#[derive(Deserialize)]
struct ManifestServers {
    #[serde(rename = "mcpServers")]
    servers: Option<IgnoredAny>,
}

#[derive(Deserialize)]
struct InlineManifest {
    #[serde(rename = "mcpServers")]
    servers: BTreeMap<String, Server>,
}

#[derive(Deserialize)]
struct PathManifest {
    #[serde(rename = "mcpServers")]
    file: String,
}

#[derive(Deserialize)]
struct ListManifest {
    #[serde(rename = "mcpServers")]
    _list: Vec<IgnoredAny>,
}

/// The manifest's own servers, an error for a broken manifest or `mcpServers`, and nothing for a list or bundle.
fn manifest_servers(root: &Path) -> Option<Result<BTreeMap<String, Server>, ()>> {
    let manifest = root.join(".claude-plugin").join("plugin.json");
    match parse::<ManifestServers>(&manifest)? {
        Err(()) => return Some(Err(())),
        Ok(ManifestServers { servers: None }) => return None,
        Ok(_) => {}
    }
    if let Some(Ok(inline)) = parse::<InlineManifest>(&manifest) {
        return Some(Ok(inline.servers));
    }
    if let Some(Ok(path)) = parse::<PathManifest>(&manifest) {
        return match json_inside(root, &path.file)? {
            Ok(file) => plugin_file(&file),
            Err(()) => Some(Err(())),
        };
    }
    if let Some(Ok(_)) = parse::<ListManifest>(&manifest) {
        return None;
    }
    Some(Err(()))
}

/// A plugin's servers from its `.mcp.json` and then its manifest, as Claude Code merges them.
pub(super) fn servers(root: &Path) -> Result<BTreeMap<String, Server>, ()> {
    let mut servers = plugin_file(&root.join(".mcp.json"))
        .transpose()?
        .unwrap_or_default();
    servers.extend(manifest_servers(root).transpose()?.unwrap_or_default());
    Ok(servers)
}

/// `None` for a missing file, `Err` for one that exists but cannot be resolved, as `parse` reports them.
fn resolve(path: &Path) -> Option<Result<PathBuf, ()>> {
    match path.canonicalize() {
        Ok(path) => Some(Ok(path)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(_) => Some(Err(())),
    }
}

// A bundle or URL is not a JSON file, and Claude Code refuses a file outside the plugin's folder.
fn json_inside(root: &Path, file: &str) -> Option<Result<PathBuf, ()>> {
    let relative = Path::new(file);
    let plain = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let json = !file.contains("://") && !file.to_lowercase().ends_with(".mcpb");
    if !(plain && json) {
        return None;
    }
    // Resolved, so a link inside the plugin cannot point the read outside it.
    let resolved = match resolve(&root.join(relative))? {
        Ok(resolved) => resolved,
        Err(()) => return Some(Err(())),
    };
    let root = match resolve(root)? {
        Ok(root) => root,
        Err(()) => return Some(Err(())),
    };
    resolved.starts_with(root).then_some(Ok(resolved))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::accounts::testdir::TestDir;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// An install entry as Claude Code writes it, with fake values in the fields the reader must skip.
    fn install(root: &Path, scope: &str, project: Option<&Path>) -> String {
        let path = serde_json::to_string(&root.to_string_lossy()).unwrap();
        let project = project.map_or(String::new(), |p| {
            format!(
                r#", "projectPath": {}"#,
                serde_json::to_string(&p.to_string_lossy()).unwrap()
            )
        });
        format!(
            r#"{{ "scope": "{scope}", "installPath": {path}, "version": "FAKE-VERSION-SECRET",
                 "gitCommitSha": "FAKE-SHA-SECRET", "installedAt": "2026-01-01T00:00:00Z"{project} }}"#
        )
    }

    fn names(enabled: &Enabled) -> Vec<&str> {
        enabled.plugins.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn installed_plugins_load_unless_switched_off_and_later_settings_win() {
        let dir = TestDir::new();
        let home = dir.path().join("acct");
        let project = dir.path().join("proj");
        let cache = home.join("plugins").join("cache");
        let mut entries = Vec::new();
        for name in ["alpha", "beta", "gamma", "delta", "quiet", "pinned", "mine"] {
            let root = cache.join("market").join(name).join("1.0.0");
            fs::create_dir_all(&root).unwrap();
            let scope_project = (name == "mine").then_some(project.as_path());
            let scope = if name == "mine" { "project" } else { "user" };
            entries.push(format!(
                r#""{name}@market": [{}]"#,
                install(&root, scope, scope_project)
            ));
        }
        write(
            &cache.join("market/quiet/1.0.0/.claude-plugin/plugin.json"),
            r#"{ "name": "quiet", "defaultEnabled": false, "author": { "email": "FAKE-EMAIL-SECRET" } }"#,
        );
        write(
            &home.join("plugins").join("installed_plugins.json"),
            &format!(
                r#"{{ "version": 2, "plugins": {{ {}, "gone@market": [{}] }} }}"#,
                entries.join(","),
                install(&dir.path().join("missing"), "user", None)
            ),
        );
        write(
            &home.join("settings.json"),
            r#"{ "env": { "TOKEN": "FAKE-ENV-SECRET" },
                 "enabledPlugins": { "beta@market": false, "gamma@market": false,
                                     "pinned@market": ["^1.0.0"], "delta@market": "huh" } }"#,
        );
        write(
            &project.join(".claude").join("settings.json"),
            r#"{ "enabledPlugins": { "gamma@market": true } }"#,
        );
        write(
            &project.join(".claude").join("settings.local.json"),
            r#"{ "enabledPlugins": { "alpha@market": false } }"#,
        );

        assert_eq!(
            names(&enabled(&home, Some(&project))),
            ["delta", "gamma", "mine", "pinned"]
        );
        assert_eq!(names(&enabled(&home, None)), ["alpha", "delta", "pinned"]);
    }

    #[test]
    fn missing_files_load_no_plugins_and_broken_ones_are_named_without_their_text() {
        let dir = TestDir::new();
        let home = dir.path().join("acct");
        let project = dir.path().join("proj");

        assert_eq!(enabled(&home, Some(&project)), Enabled::default());
        assert!(synced_skills_on(&home));

        write(
            &home.join("plugins").join("installed_plugins.json"),
            r#"{ "plugins": "FAKE-SECRET" "#,
        );
        write(
            &home.join("settings.json"),
            r#"{ "env": "FAKE-ENV-SECRET" "#,
        );
        write(
            &project.join(".claude").join("settings.local.json"),
            r#"{ "enabledPlugins": "FAKE-SECRET" }"#,
        );

        let found = enabled(&home, Some(&project));

        assert!(found.plugins.is_empty());
        assert_eq!(
            found.unreadable,
            [
                "the account's settings.json",
                ".claude/settings.local.json",
                "the account's installed_plugins.json",
            ]
        );
        assert!(!format!("{found:?}").contains("FAKE"));
        assert!(synced_skills_on(&home));
    }

    #[test]
    fn the_settings_can_turn_synced_skills_off() {
        let dir = TestDir::new();
        let home = dir.path().join("acct");
        write(
            &home.join("settings.json"),
            r#"{ "syncClaudeAiSkills": false }"#,
        );

        assert!(!synced_skills_on(&home));
    }

    // Symlinks need extra rights on Windows, so the macOS leg covers this one.
    #[cfg(unix)]
    #[test]
    fn a_manifest_path_that_links_outside_the_plugin_is_not_read() {
        let dir = TestDir::new();
        let root = dir.path().join("plugin");
        write(
            &dir.path().join("outside.json"),
            r#"{ "mcpServers": { "FAKE-OUTSIDE": {} } }"#,
        );
        write(
            &root.join("inside.json"),
            r#"{ "mcpServers": { "ok": {} } }"#,
        );
        std::os::unix::fs::symlink(dir.path().join("outside.json"), root.join("link.json"))
            .unwrap();
        let manifest = root.join(".claude-plugin").join("plugin.json");

        write(&manifest, r#"{ "mcpServers": "./link.json" }"#);
        assert!(servers(&root).unwrap().is_empty());

        write(&manifest, r#"{ "mcpServers": "./inside.json" }"#);
        assert_eq!(
            servers(&root).unwrap().into_keys().collect::<Vec<_>>(),
            ["ok"]
        );

        write(&manifest, r#"{ "mcpServers": "./gone.json" }"#);
        assert!(servers(&root).unwrap().is_empty());
    }

    // A folder the user cannot search is the portable way to make resolving fail other than by absence.
    #[cfg(unix)]
    #[test]
    fn a_manifest_path_that_cannot_be_resolved_is_named_unreadable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let root = dir.path().join("plugin");
        let locked = root.join("locked");
        write(&locked.join("servers.json"), r#"{ "mcpServers": {} }"#);
        write(
            &root.join(".claude-plugin").join("plugin.json"),
            r#"{ "mcpServers": "./locked/servers.json" }"#,
        );
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let found = servers(&root);

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(found.is_err());
    }
}
