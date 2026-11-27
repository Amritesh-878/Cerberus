//! An AppImage's `AppRun` prepends the image's folders to `LD_LIBRARY_PATH` and others and saves no old value, so children lose those entries.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use portable_pty::CommandBuilder;

/// About the image itself, set by the AppImage runtime; nothing started from a tab is in the image.
const IMAGE_VARS: [&str; 4] = ["APPIMAGE", "APPDIR", "ARGV0", "OWD"];
/// Set outright by `AppRun` and the GTK hook, over any value the user had.
const SET_OUTRIGHT: [&str; 2] = ["PYTHONDONTWRITEBYTECODE", "GTK_THEME"];

/// One variable to set (`Some`) or unset (`None`).
pub type Change = (String, Option<String>);

/// What to change in `vars` to undo the AppImage's start scripts; nothing unless `APPIMAGE` and `APPDIR` are set.
pub fn changes(vars: &[(String, String)]) -> Vec<Change> {
    let get = |name: &str| vars.iter().find(|(n, _)| n == name).map(|(_, v)| v);
    let (Some(_), Some(appdir)) = (get("APPIMAGE"), get("APPDIR")) else {
        return Vec::new();
    };
    let appdir = Path::new(appdir);
    vars.iter()
        .filter_map(|(name, value)| {
            if IMAGE_VARS.contains(&name.as_str()) || SET_OUTRIGHT.contains(&name.as_str()) {
                return Some((name.clone(), None));
            }
            outside(name, value, appdir).map(|kept| (name.clone(), kept))
        })
        .collect()
}

/// A `:` list without its entries inside the image, `None` when it has none there.
fn outside(name: &str, value: &str, appdir: &Path) -> Option<Option<String>> {
    let entries: Vec<&str> = value.split(':').collect();
    let inside = |entry: &&str| !entry.is_empty() && Path::new(entry).starts_with(appdir);
    if !entries.iter().any(inside) {
        return None;
    }
    let lead = entries.iter().take_while(|e| inside(e)).count();
    // The GTK hook adds `/usr/share` right after the image's own folders.
    let hook_added =
        (name == "XDG_DATA_DIRS" && entries.get(lead) == Some(&"/usr/share")).then_some(lead);
    let kept: Vec<&str> = entries
        .iter()
        .enumerate()
        .filter(|&(at, entry)| Some(at) != hook_added && !entry.is_empty() && !inside(entry))
        .map(|(_, entry)| *entry)
        .collect();
    Some((!kept.is_empty()).then(|| kept.join(":")))
}

/// A process with the user's environment, for anything Cerberus starts outside a tab.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for (name, value) in changes(&own_vars()) {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
}

/// Applied last, after the tab's own variables, since a wrapper's `PATH` is built from the app's.
pub fn restore(builder: &mut CommandBuilder) {
    let vars: Vec<(String, String)> = builder
        .iter_full_env_as_str()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    for (name, value) in changes(&vars) {
        match value {
            Some(value) => builder.env(name, value),
            None => builder.env_remove(name),
        }
    }
}

// A variable that isn't Unicode was not set by the start scripts, so it is left alone.
fn own_vars() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPDIR: &str = "/tmp/.mount_CerberAb12";

    fn image_env(extra: &[(&str, &str)]) -> Vec<(String, String)> {
        // What AppRun and the GTK hook leave for a user whose own values are `/usr/lib/extra` and the like.
        let d = APPDIR;
        let mut vars = vec![
            ("APPIMAGE", "/home/u/Apps/Cerberus.AppImage".to_owned()),
            ("APPDIR", d.to_owned()),
            ("ARGV0", "Cerberus.AppImage".to_owned()),
            ("OWD", "/home/u".to_owned()),
            (
                "LD_LIBRARY_PATH",
                format!("{d}/usr/lib/:{d}/usr/lib/x86_64-linux-gnu/:{d}/lib64/:"),
            ),
            (
                "PATH",
                format!("{d}/usr/bin/:{d}/usr/sbin/:{d}/bin/:/home/u/.local/bin:/usr/bin"),
            ),
            ("PYTHONHOME", format!("{d}/usr/")),
            ("PYTHONPATH", format!("{d}/usr/share/pyshared/:/opt/py")),
            ("PYTHONDONTWRITEBYTECODE", "1".to_owned()),
            (
                "XDG_DATA_DIRS",
                format!(
                    "{d}/usr/share/:{d}/usr/share:/usr/share:/usr/local/share:/var/lib/flatpak/exports/share"
                ),
            ),
            (
                "GSETTINGS_SCHEMA_DIR",
                format!("{d}/usr/share/glib-2.0/schemas/:"),
            ),
            ("GIO_MODULE_DIR", format!("{d}//usr/lib/gio/modules")),
            (
                "GDK_PIXBUF_MODULE_FILE",
                format!("{d}//usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache"),
            ),
            (
                "GST_PLUGIN_SYSTEM_PATH_1_0",
                format!("{d}/usr/lib/gstreamer-1.0:"),
            ),
            (
                "PERLLIB",
                format!("{d}/usr/share/perl5/:{d}/usr/lib/perl5/:"),
            ),
            ("GTK_THEME", "Adwaita:dark".to_owned()),
            ("HOME", "/home/u".to_owned()),
            ("LS_COLORS", "di=01;34:ln=01;36".to_owned()),
        ];
        vars.extend(extra.iter().map(|&(n, v)| (n, v.to_owned())));
        vars.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()
    }

    fn after(vars: &[(String, String)]) -> Vec<(String, String)> {
        let changes = changes(vars);
        vars.iter()
            .filter_map(
                |(name, value)| match changes.iter().find(|(n, _)| n == name) {
                    Some((_, Some(new))) => Some((name.clone(), new.clone())),
                    Some((_, None)) => None,
                    None => Some((name.clone(), value.clone())),
                },
            )
            .collect()
    }

    fn value<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
        vars.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn no_variable_keeps_an_entry_inside_the_image() {
        let cleaned = after(&image_env(&[]));

        for (name, value) in &cleaned {
            assert!(!value.contains(APPDIR), "{name}={value}");
        }
        assert_eq!(value(&cleaned, "PATH"), Some("/home/u/.local/bin:/usr/bin"));
        assert_eq!(value(&cleaned, "PYTHONPATH"), Some("/opt/py"));
        assert_eq!(value(&cleaned, "HOME"), Some("/home/u"));
        assert_eq!(value(&cleaned, "LS_COLORS"), Some("di=01;34:ln=01;36"));
    }

    #[test]
    fn a_list_the_image_alone_filled_is_unset() {
        let cleaned = after(&image_env(&[]));

        for name in [
            "LD_LIBRARY_PATH",
            "PYTHONHOME",
            "GSETTINGS_SCHEMA_DIR",
            "GIO_MODULE_DIR",
            "GDK_PIXBUF_MODULE_FILE",
            "GST_PLUGIN_SYSTEM_PATH_1_0",
            "PERLLIB",
            "APPIMAGE",
            "APPDIR",
            "ARGV0",
            "OWD",
            "PYTHONDONTWRITEBYTECODE",
            "GTK_THEME",
        ] {
            assert_eq!(value(&cleaned, name), None, "{name}");
        }
    }

    #[test]
    fn the_users_own_library_path_survives() {
        let mut vars = image_env(&[]);
        vars.retain(|(n, _)| n != "LD_LIBRARY_PATH");
        vars.push((
            "LD_LIBRARY_PATH".to_owned(),
            format!("{APPDIR}/usr/lib/:{APPDIR}/lib64/:/opt/cuda/lib64"),
        ));

        assert_eq!(
            value(&after(&vars), "LD_LIBRARY_PATH"),
            Some("/opt/cuda/lib64")
        );
    }

    #[test]
    fn the_gtk_hooks_usr_share_goes_but_the_users_own_stays() {
        let cleaned = after(&image_env(&[]));
        assert_eq!(
            value(&cleaned, "XDG_DATA_DIRS"),
            Some("/usr/local/share:/var/lib/flatpak/exports/share")
        );

        let mut unset = image_env(&[]);
        unset.retain(|(n, _)| n != "XDG_DATA_DIRS");
        unset.push((
            "XDG_DATA_DIRS".to_owned(),
            format!("{APPDIR}/usr/share/:{APPDIR}/usr/share:/usr/share:"),
        ));
        assert_eq!(value(&after(&unset), "XDG_DATA_DIRS"), None);
    }

    #[test]
    fn a_users_xdg_data_dirs_without_usr_share_comes_back_as_it_was() {
        let mut vars = image_env(&[]);
        vars.retain(|(n, _)| n != "XDG_DATA_DIRS");
        // The hook's `/usr/share` always follows the image's folders, so the rest is the user's own value.
        vars.push((
            "XDG_DATA_DIRS".to_owned(),
            format!(
                "{APPDIR}/usr/share/:{APPDIR}/usr/share:/usr/share:/opt/share:/usr/local/share"
            ),
        ));

        assert_eq!(
            value(&after(&vars), "XDG_DATA_DIRS"),
            Some("/opt/share:/usr/local/share")
        );
    }

    #[test]
    fn a_folder_that_only_starts_like_the_image_is_kept() {
        let vars = image_env(&[("MANPATH", "/tmp/.mount_CerberAb12x/share/man")]);

        assert_eq!(
            value(&after(&vars), "MANPATH"),
            Some("/tmp/.mount_CerberAb12x/share/man")
        );
    }

    #[test]
    fn outside_an_appimage_nothing_changes() {
        let mut vars = image_env(&[]);
        vars.retain(|(n, _)| n != "APPIMAGE");

        assert!(changes(&vars).is_empty());
    }

    #[test]
    fn a_tab_started_from_an_appimage_gets_no_entry_inside_it() {
        let mut builder = CommandBuilder::new("/bin/sh");
        builder.env_clear();
        for (name, value) in image_env(&[]) {
            builder.env(name, value);
        }

        restore(&mut builder);

        let vars: Vec<(&str, &str)> = builder.iter_full_env_as_str().collect();
        assert!(!vars.is_empty());
        for (name, value) in vars {
            assert!(!value.contains(APPDIR), "{name}={value}");
            assert_ne!(name, "LD_LIBRARY_PATH");
        }
    }
}
