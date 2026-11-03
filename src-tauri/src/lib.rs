mod accounts;
mod cli_link;
mod commands;
mod handover;
mod job;
#[cfg(unix)]
mod login_path;
mod menu;
mod open;
mod programs;
mod pty;
mod saved;
mod sessions;

use std::sync::Arc;

use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager, RunEvent};

use crate::accounts::Accounts;
use crate::handover::RunningTabs;
use crate::pty::PtyManager;
use crate::saved::Saved;

pub use crate::open::console;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(unix)]
    login_path::adopt();
    let context = tauri::generate_context!();
    let command = open::from_command_line(&context.config().identifier);
    // Joined before anything starts, so the webview and every tab are in the job too.
    if let Err(e) = job::end_children_with_app() {
        eprintln!("tab programs may outlive a crash: {e}");
    }
    let app = menu::with_app_menu(tauri::Builder::default())
        .plugin(tauri_plugin_single_instance::init(open::on_second_launch))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(PtyManager::default())
        .manage(Arc::new(RunningTabs::default()))
        .manage(open::Pending::default())
        .setup(move |app| {
            let paths = app.path();
            let config_dir = paths.app_config_dir()?;
            app.manage(Accounts::new(&config_dir, paths.home_dir()?));
            let handle = app.handle().clone();
            let report = move |failure: Option<&str>| {
                // Fails only once the window is gone, and then nobody is left to show it.
                let _ = handle.emit("tree-save", failure);
            };
            app.manage(Saved::open(&config_dir.join(saved::FILE_NAME), report));
            if let open::Command::Open(spec) = &command {
                open::request(app.handle(), spec);
            }
            open::listen(app.handle());
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
            commands::resume_chat,
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
            commands::set_sidebar_width,
            commands::set_alerts,
            commands::account_status,
            commands::tree_load,
            commands::tree_save,
            commands::continue_on_account,
            commands::install_cli,
            open::launch::open_take,
        ])
        .build(context)
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<PtyManager>().kill_all();
            handle.state::<Saved>().flush();
            job::spare_children_on_exit();
        }
    });
}
