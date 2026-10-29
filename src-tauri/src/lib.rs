mod accounts;
mod commands;
mod job;
mod programs;
mod pty;
mod saved;
mod sessions;

use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager, RunEvent};

use crate::accounts::Accounts;
use crate::pty::PtyManager;
use crate::saved::Saved;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Joined before anything starts, so the webview and every tab are in the job too.
    if let Err(e) = job::end_children_with_app() {
        eprintln!("tab programs may outlive a crash: {e}");
    }
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(PtyManager::default())
        .setup(|app| {
            let paths = app.path();
            let config_dir = paths.app_config_dir()?;
            app.manage(Accounts::new(&config_dir, paths.home_dir()?));
            let handle = app.handle().clone();
            let report = move |failure: Option<&str>| {
                // Fails only once the window is gone, and then nobody is left to show it.
                let _ = handle.emit("tree-save", failure);
            };
            app.manage(Saved::open(&config_dir.join("tree.db"), report));
            Ok(())
        })
        // A reload drops the page that owned every session, so nothing could close them later.
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Started {
                webview.state::<PtyManager>().kill_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_programs,
            commands::pty_spawn,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_ack,
            commands::pty_kill,
            commands::load_profiles,
            commands::reset_profiles,
            commands::detect_accounts,
            commands::suggest_config_dir,
            commands::add_account,
            commands::rename_account,
            commands::remove_account,
            commands::set_default_account,
            commands::set_start_folder,
            commands::set_shell,
            commands::account_status,
            commands::tree_load,
            commands::tree_save,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<PtyManager>().kill_all();
            handle.state::<Saved>().flush();
            job::spare_children_on_exit();
        }
    });
}
