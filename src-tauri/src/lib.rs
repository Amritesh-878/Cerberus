mod accounts;
mod commands;
mod programs;
mod pty;

use tauri::webview::PageLoadEvent;
use tauri::{Manager, RunEvent};

use crate::accounts::Accounts;
use crate::pty::PtyManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(PtyManager::default())
        .setup(|app| {
            let paths = app.path();
            app.manage(Accounts::new(&paths.app_config_dir()?, paths.home_dir()?));
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
            commands::set_defaults,
            commands::account_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<PtyManager>().kill_all();
        }
    });
}
