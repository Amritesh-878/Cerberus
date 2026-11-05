//! What a `claude` tab is started with so its chat gets the `cerberus` MCP server.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::json;

pub const SEND_TOOL: &str = "mcp__cerberus__send_to_tab";
const CONFIG_FILE: &str = "mcp.json";
// Claude Code caps a server's call timeout at the largest 32-bit millisecond count, ~24 days.
const CALL_TIMEOUT_MS: u64 = 2_147_483_647;

/// The MCP config every `claude` tab is given; `None` when the helper is missing, so tabs start as before.
#[derive(Debug, Clone, Default)]
pub struct Messages {
    config: Option<PathBuf>,
}

impl Messages {
    /// Writes the config beside the app's data, naming the helper that ships beside the app.
    pub fn prepare(data_dir: &Path) -> Self {
        let Some(helper) = helper_path() else {
            eprintln!(
                "bridge: the cerberus-mcp helper is missing, so messages between accounts are off"
            );
            return Self::default();
        };
        match write_config(data_dir, &helper) {
            Ok(config) => Self {
                config: Some(config),
            },
            Err(e) => {
                eprintln!("bridge: could not write the MCP config: {e}");
                Self::default()
            }
        }
    }

    pub fn ready(&self) -> bool {
        self.config.is_some()
    }

    /// The tab's arguments with the server added; the user's own MCP servers still load beside it.
    pub fn args(&self, args: &[String]) -> Vec<String> {
        let Some(config) = &self.config else {
            return args.to_vec();
        };
        // The `=` form, since both flags take several values and would swallow what follows.
        let mut with = vec![
            format!("--mcp-config={}", config.display()),
            format!("--allowedTools={SEND_TOOL}"),
        ];
        with.extend_from_slice(args);
        with
    }
}

fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let helper = exe.with_file_name(format!("cerberus-mcp{}", std::env::consts::EXE_SUFFIX));
    helper.is_file().then_some(helper)
}

fn write_config(dir: &Path, helper: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(CONFIG_FILE);
    let config = json!({
        "mcpServers": {
            "cerberus": {
                "type": "stdio",
                "command": helper,
                "args": [],
                "timeout": CALL_TIMEOUT_MS,
                // In the first prompt rather than behind tool search, so the chat knows it can send.
                "alwaysLoad": true
            }
        }
    });
    fs::write(&path, serde_json::to_vec_pretty(&config)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_messages_on_the_server_and_its_send_tool_come_first() {
        let messages = Messages {
            config: Some(PathBuf::from("/data/mcp.json")),
        };
        let user = ["--model".to_owned(), "opus".to_owned()];

        let args = messages.args(&user);

        assert_eq!(
            args,
            [
                "--mcp-config=/data/mcp.json",
                "--allowedTools=mcp__cerberus__send_to_tab",
                "--model",
                "opus"
            ]
        );
        assert!(!args.iter().any(|a| a.contains("strict-mcp-config")));
    }

    #[test]
    fn without_the_helper_the_arguments_are_left_alone() {
        let user = ["--resume".to_owned(), "abc".to_owned()];

        assert_eq!(Messages::default().args(&user), user);
        assert!(!Messages::default().ready());
    }

    #[test]
    fn the_config_names_the_helper_with_a_long_timeout_and_no_secret() {
        let dir = std::env::temp_dir().join(format!("cerberus-mcp-config-{}", std::process::id()));

        let path = write_config(&dir, Path::new("/app/cerberus-mcp")).unwrap();
        let written: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        let server = &written["mcpServers"]["cerberus"];
        assert_eq!(server["command"], "/app/cerberus-mcp");
        assert_eq!(server["timeout"], CALL_TIMEOUT_MS);
        assert_eq!(server["alwaysLoad"], true);
        assert!(server.get("env").is_none());
    }
}
