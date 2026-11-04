use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where a server is configured, which is also Claude Code's order of precedence, last wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    User,
    Project,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connector {
    pub name: String,
    /// `stdio`, `http`, `sse` or `ws`; anything else shows as `other`.
    pub kind: &'static str,
    pub scope: Scope,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connectors {
    pub servers: Vec<Connector>,
    /// Files that exist but could not be read, named without their content.
    pub unreadable: Vec<&'static str>,
}

// Only names and `type` are parsed; commands, args, URLs, headers and env hold tokens and are never read into memory.
#[derive(Deserialize)]
struct Server {
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize, Default)]
struct Servers {
    #[serde(default, rename = "mcpServers")]
    servers: BTreeMap<String, Server>,
}

#[derive(Deserialize, Default)]
struct AccountConfig {
    #[serde(default, rename = "mcpServers")]
    servers: BTreeMap<String, Server>,
    #[serde(default)]
    projects: BTreeMap<String, Servers>,
}

fn kind(server: &Server) -> &'static str {
    match server.kind.as_deref() {
        None | Some("stdio") => "stdio",
        Some("http") => "http",
        Some("sse") => "sse",
        Some("ws") => "ws",
        Some(_) => "other",
    }
}

/// `None` for a missing file; `Err` hides why, since a parse error can quote the file.
fn parse<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<Result<T, ()>> {
    match fs::read(path) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).map_err(|_| ())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(_) => Some(Err(())),
    }
}

/// The servers a `claude` started in `folder` with this account config would load.
pub fn read(account_config: &Path, folder: Option<&Path>) -> Connectors {
    let mut found = Connectors::default();
    let mut by_name: BTreeMap<String, Connector> = BTreeMap::new();
    let mut add = |servers: BTreeMap<String, Server>, scope: Scope| {
        for (name, server) in servers {
            let connector = Connector {
                kind: kind(&server),
                name: name.clone(),
                scope,
            };
            let keep = by_name.get(&name).is_none_or(|known| known.scope <= scope);
            if keep {
                by_name.insert(name, connector);
            }
        }
    };
    match parse::<AccountConfig>(account_config) {
        Some(Ok(config)) => {
            add(config.servers, Scope::User);
            let here = folder.and_then(|folder| {
                config
                    .projects
                    .into_iter()
                    .find(|(key, _)| same_folder(key, folder))
            });
            if let Some((_, project)) = here {
                add(project.servers, Scope::Local);
            }
        }
        Some(Err(())) => found.unreadable.push("the account's .claude.json"),
        None => {}
    }
    if let Some(folder) = folder {
        match parse::<Servers>(&folder.join(".mcp.json")) {
            Some(Ok(project)) => add(project.servers, Scope::Project),
            Some(Err(())) => found.unreadable.push(".mcp.json"),
            None => {}
        }
    }
    found.servers = by_name.into_values().collect();
    found
}

// Claude Code keys projects by path with forward slashes; Windows paths compare without case.
fn same_folder(key: &str, folder: &Path) -> bool {
    let norm = |s: &str| {
        let s = s.replace('\\', "/");
        let s = s.trim_end_matches('/');
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.to_owned()
        }
    };
    norm(key) == norm(&folder.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testdir::TestDir;

    const SECRETS: [&str; 6] = [
        "FAKE-CMD-SECRET",
        "FAKE-ARG-SECRET",
        "FAKE-URL-SECRET",
        "FAKE-HEADER-SECRET",
        "FAKE-ENV-SECRET",
        "fake-account@example.com",
    ];

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn fixture(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
        let project = dir.path().join("proj");
        let key = project.to_string_lossy().replace('\\', "/");
        let config = dir.path().join("acct").join(".claude.json");
        write(
            &config,
            &format!(
                r#"{{
                "oauthAccount": {{ "emailAddress": "fake-account@example.com" }},
                "mcpServers": {{
                    "browser": {{ "type": "stdio", "command": "FAKE-CMD-SECRET", "args": ["FAKE-ARG-SECRET"] }},
                    "tracker": {{ "type": "http", "url": "https://x.test/?k=FAKE-URL-SECRET",
                                  "headers": {{ "Authorization": "Bearer FAKE-HEADER-SECRET" }} }},
                    "shared": {{ "type": "sse", "url": "https://x.test/sse" }}
                }},
                "projects": {{
                    "{key}": {{
                        "allowedTools": ["Bash"],
                        "mcpServers": {{ "db": {{ "command": "pg", "env": {{ "PGPASSWORD": "FAKE-ENV-SECRET" }} }} }}
                    }},
                    "C:/elsewhere": {{ "mcpServers": {{ "not-here": {{ "type": "http" }} }} }}
                }}
            }}"#
            ),
        );
        write(
            &project.join(".mcp.json"),
            r#"{ "mcpServers": {
                "shared": { "type": "http", "url": "https://x.test/FAKE-URL-SECRET" },
                "socket": { "type": "carrier-pigeon" } } }"#,
        );
        (config, project)
    }

    #[test]
    fn project_user_and_local_servers_are_listed_by_name_and_kind() {
        let dir = TestDir::new();
        let (config, project) = fixture(&dir);

        let found = read(&config, Some(&project));

        let listed: Vec<(&str, &str, Scope)> = found
            .servers
            .iter()
            .map(|c| (c.name.as_str(), c.kind, c.scope))
            .collect();
        assert_eq!(
            listed,
            [
                ("browser", "stdio", Scope::User),
                ("db", "stdio", Scope::Local),
                ("shared", "http", Scope::Project),
                ("socket", "other", Scope::Project),
                ("tracker", "http", Scope::User),
            ]
        );
        assert!(found.unreadable.is_empty());
    }

    #[test]
    fn no_command_url_header_or_env_value_reaches_the_output() {
        let dir = TestDir::new();
        let (config, project) = fixture(&dir);

        let shown = serde_json::to_string(&read(&config, Some(&project))).unwrap();
        let debugged = format!("{:?}", read(&config, Some(&project)));

        for secret in SECRETS {
            assert!(!shown.contains(secret), "{secret} in {shown}");
            assert!(!debugged.contains(secret), "{secret} in {debugged}");
        }
    }

    #[test]
    fn without_a_folder_only_the_user_servers_load() {
        let dir = TestDir::new();
        let (config, _) = fixture(&dir);

        let names: Vec<String> = read(&config, None)
            .servers
            .into_iter()
            .map(|c| c.name)
            .collect();

        assert_eq!(names, ["browser", "shared", "tracker"]);
    }

    #[test]
    fn missing_files_are_empty_and_broken_ones_are_named_without_their_text() {
        let dir = TestDir::new();
        let project = dir.path().join("proj");
        let config = dir.path().join(".claude.json");

        assert_eq!(read(&config, Some(&project)), Connectors::default());

        write(&config, r#"{ "mcpServers": { "x": "FAKE-CMD-SECRET" "#);
        write(&project.join(".mcp.json"), "FAKE-URL-SECRET");
        let found = read(&config, Some(&project));

        assert!(found.servers.is_empty());
        assert_eq!(
            found.unreadable,
            ["the account's .claude.json", ".mcp.json"]
        );
        let shown = serde_json::to_string(&found).unwrap();
        assert!(!shown.contains("FAKE"), "{shown}");
    }

    #[test]
    fn project_keys_match_however_the_folder_is_spelled() {
        assert!(same_folder(
            "/home/u/code/app",
            Path::new("/home/u/code/app/")
        ));
        #[cfg(windows)]
        assert!(same_folder(
            "C:/Users/u/Code/App",
            Path::new(r"c:\users\u\code\app")
        ));
        assert!(!same_folder(
            "/home/u/code/app2",
            Path::new("/home/u/code/app")
        ));
    }
}
