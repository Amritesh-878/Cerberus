use std::sync::OnceLock;

use serde::Deserialize;

use crate::accounts::Account;

// The webview reads the same file for the colours, so the names can never drift apart.
const PALETTE_JSON: &str = include_str!("../../../src/palette.json");

/// How many colours the id-based fallback cycles through, from before colours could be chosen.
const LEGACY_COLOURS: usize = 5;

#[derive(Deserialize)]
struct Swatch {
    name: String,
}

fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        // Built in and parsed by a test, so this cannot fail in a release; empty would only mean no colours.
        let swatches: Vec<Swatch> = serde_json::from_str(PALETTE_JSON).unwrap_or_default();
        swatches.into_iter().map(|s| s.name).collect()
    })
}

pub fn is_known(name: &str) -> bool {
    names().iter().any(|n| n == name)
}

/// The colour an account shows: its saved one, else the one its id gave it before colours were saved.
fn shown(account: &Account) -> Option<&'static str> {
    account
        .color
        .as_deref()
        .and_then(|saved| names().iter().find(|n| *n == saved))
        .or_else(|| {
            let index = usize::try_from(account.id).ok()? % LEGACY_COLOURS;
            names().get(index)
        })
        .map(String::as_str)
}

/// The first colour no account shows, else the first colour, so new accounts stay apart while they can.
pub fn first_free(accounts: &[Account]) -> Option<String> {
    let taken: Vec<_> = accounts.iter().filter_map(shown).collect();
    names()
        .iter()
        .find(|n| !taken.contains(&n.as_str()))
        .or_else(|| names().first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn account(id: u32, color: Option<&str>) -> Account {
        Account {
            id,
            name: format!("a{id}"),
            config_dir: PathBuf::from(format!(r"C:\h\.claude-{id}")),
            color: color.map(str::to_owned),
        }
    }

    #[test]
    fn the_palette_has_eight_named_colours() {
        assert_eq!(names().len(), 8);
        assert!(is_known("terracotta") && is_known("silver"));
        assert!(!is_known("#d97757"));
    }

    #[test]
    fn an_account_with_no_saved_colour_takes_the_one_its_id_gave_it() {
        assert_eq!(shown(&account(6, None)), Some("blue"));
        assert_eq!(shown(&account(6, Some("teal"))), Some("teal"));
        assert_eq!(shown(&account(6, Some("unknown"))), Some("blue"));
    }

    #[test]
    fn the_first_free_colour_skips_the_ones_shown() {
        let accounts = [account(0, None), account(1, Some("green"))];

        assert_eq!(first_free(&accounts).as_deref(), Some("blue"));
    }

    #[test]
    fn with_every_colour_taken_the_first_one_is_reused() {
        let names = [
            "terracotta",
            "blue",
            "green",
            "purple",
            "yellow",
            "rose",
            "teal",
            "silver",
        ];
        let accounts: Vec<_> = (0..8)
            .map(|i| account(i, Some(names[i as usize])))
            .collect();

        assert_eq!(first_free(&accounts).as_deref(), Some("terracotta"));
    }
}
