use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

const MOST_VARS: usize = 64;
const LONGEST_NAME: usize = 128;
const LONGEST_VALUE: usize = 32 * 1024;

// Set or stripped by Cerberus for every tab; a tab's own value would move it off its account or bridge.
const MANAGED: [&str; 13] = [
    "CLAUDE_CONFIG_DIR",
    "TERM",
    "COLORTERM",
    "ZDOTDIR",
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "AI_AGENT",
];
const MANAGED_PREFIXES: [&str; 2] = ["CERBERUS_", "CLAUDE_CODE_MESSAGING_"];

/// Errors name the variable at most, never its value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvError {
    #[error("{0:?} is not a variable name: use letters, digits and _, not starting with a digit")]
    BadName(String),
    #[error("{0} is set by Cerberus, so a tab cannot change it")]
    Managed(String),
    #[error("the value of {0} is too long or holds a NUL character")]
    BadValue(String),
    #[error("a tab holds at most {MOST_VARS} variables")]
    TooMany,
    #[error("this tab is not saved")]
    NoTab,
}

impl Serialize for EnvError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// A tab's variable; its value never prints, so a debug line or a panic cannot leak it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for EnvVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}=<hidden>", self.name)
    }
}

/// One change from the dialog: a value sets the variable, `None` removes it.
#[derive(Clone, Deserialize)]
pub struct EnvEdit {
    pub name: String,
    pub value: Option<String>,
}

impl fmt::Debug for EnvEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let action = if self.value.is_some() { "set" } else { "unset" };
        write!(f, "{action} {}", self.name)
    }
}

/// Windows names ignore case, so a managed name is refused in any case on every platform.
pub fn check_name(name: &str) -> Result<(), EnvError> {
    let mut chars = name.chars();
    let starts_well = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !starts_well
        || name.len() > LONGEST_NAME
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(EnvError::BadName(name.chars().take(LONGEST_NAME).collect()));
    }
    let upper = name.to_ascii_uppercase();
    if MANAGED.contains(&upper.as_str()) || MANAGED_PREFIXES.iter().any(|p| upper.starts_with(p)) {
        return Err(EnvError::Managed(name.to_owned()));
    }
    Ok(())
}

fn check(var: &EnvVar) -> Result<(), EnvError> {
    check_name(&var.name)?;
    if var.value.len() > LONGEST_VALUE || var.value.contains('\0') {
        return Err(EnvError::BadValue(var.name.clone()));
    }
    Ok(())
}

/// `current` with the edits made in order, each name kept once and matched without case.
pub fn edited(current: &[EnvVar], edits: Vec<EnvEdit>) -> Result<Vec<EnvVar>, EnvError> {
    let mut vars = current.to_vec();
    for edit in edits {
        vars.retain(|v| !v.name.eq_ignore_ascii_case(&edit.name));
        match edit.value {
            Some(value) => {
                let var = EnvVar {
                    name: edit.name,
                    value,
                };
                check(&var)?;
                vars.push(var);
            }
            None => check_name(&edit.name).or_else(|e| match e {
                // A name saved before it became managed can still be removed.
                EnvError::Managed(_) => Ok(()),
                other => Err(other),
            })?,
        }
    }
    if vars.len() > MOST_VARS {
        return Err(EnvError::TooMany);
    }
    Ok(vars)
}

/// Read back from the tabs file: anything that would now be refused is dropped rather than applied.
pub fn kept(vars: Vec<EnvVar>) -> Vec<EnvVar> {
    vars.into_iter()
        .filter(|v| check(v).is_ok())
        .take(MOST_VARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "sk-FAKE-env-value-123";

    fn set(name: &str, value: &str) -> EnvEdit {
        EnvEdit {
            name: name.to_owned(),
            value: Some(value.to_owned()),
        }
    }

    #[test]
    fn names_are_checked_and_managed_names_refused_in_any_case() {
        for good in ["API_URL", "_x", "a1"] {
            assert_eq!(check_name(good), Ok(()), "{good}");
        }
        for bad in ["", "1A", "A-B", "A B", "A=B", "É"] {
            assert!(
                matches!(check_name(bad), Err(EnvError::BadName(_))),
                "{bad}"
            );
        }
        for managed in [
            "CLAUDE_CONFIG_DIR",
            "claude_config_dir",
            "CLAUDE_CODE_MESSAGING_SOCKET",
            "CLAUDE_CODE_MESSAGING_TOKEN",
            "CERBERUS_BRIDGE",
            "Cerberus_Bridge_Token",
            "CERBERUS_USER_ZDOTDIR",
            "ZDOTDIR",
            "TERM",
            "CLAUDECODE",
        ] {
            assert!(
                matches!(check_name(managed), Err(EnvError::Managed(_))),
                "{managed}"
            );
        }
    }

    #[test]
    fn edits_set_replace_and_remove_without_case() {
        let vars = edited(&[], vec![set("API_URL", "a"), set("MODE", "x")]).unwrap();
        let vars = edited(
            &vars,
            vec![
                set("api_url", "b"),
                EnvEdit {
                    name: "MODE".to_owned(),
                    value: None,
                },
            ],
        )
        .unwrap();

        assert_eq!(
            vars,
            [EnvVar {
                name: "api_url".to_owned(),
                value: "b".to_owned()
            }]
        );
    }

    #[test]
    fn a_refused_edit_changes_nothing_and_never_names_the_value() {
        let errors = [
            edited(&[], vec![set("CLAUDE_CONFIG_DIR", SECRET)]).unwrap_err(),
            edited(&[], vec![set("BAD NAME", SECRET)]).unwrap_err(),
            edited(&[], vec![set("OK", &format!("{SECRET}\0"))]).unwrap_err(),
        ];

        for error in errors {
            let shown = format!(
                "{error} {error:?} {}",
                serde_json::to_string(&error).unwrap()
            );
            assert!(!shown.contains(SECRET), "{shown}");
        }
    }

    #[test]
    fn values_never_reach_debug_output() {
        let var = EnvVar {
            name: "TOKEN".to_owned(),
            value: SECRET.to_owned(),
        };
        let edit = set("TOKEN", SECRET);

        let shown = format!("{var:?} {edit:?} {:?}", [&var]);

        assert!(!shown.contains(SECRET));
        assert!(shown.contains("TOKEN"));
    }

    #[test]
    fn too_many_variables_are_refused() {
        let edits = (0..=MOST_VARS)
            .map(|n| set(&format!("V{n}"), "1"))
            .collect();

        assert_eq!(edited(&[], edits), Err(EnvError::TooMany));
    }

    #[test]
    fn a_managed_name_already_saved_can_be_removed_but_is_never_applied() {
        let old = vec![EnvVar {
            name: "CLAUDE_CONFIG_DIR".to_owned(),
            value: "x".to_owned(),
        }];

        assert!(kept(old.clone()).is_empty());
        let removal = EnvEdit {
            name: "CLAUDE_CONFIG_DIR".to_owned(),
            value: None,
        };
        assert_eq!(edited(&old, vec![removal]), Ok(Vec::new()));
    }
}
