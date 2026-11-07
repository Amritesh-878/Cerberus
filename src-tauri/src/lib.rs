mod accounts;
mod bridge;
mod cli_link;
mod commands;
mod handover;
mod job;
#[cfg(unix)]
mod login_path;
mod menu;
mod messages;
mod open;
mod panel;
mod programs;
mod pty;
mod run;
mod saved;
mod sessions;
mod suggestions;

use std::sync::Arc;

use tauri::webview::PageLoadEvent;
use tauri::{Emitter, Manager, RunEvent};

use crate::accounts::Accounts;
use crate::bridge::Bridge;
use crate::bridge::spawn::Messages;
use crate::handover::RunningTabs;
use crate::panel::PrCache;
use crate::pty::PtyManager;
use crate::saved::Saved;
use crate::suggestions::Suggestions;

pub use crate::open::console;

/// The `cerberus-mcp` helper's whole life: MCP on stdio until its `claude` closes it.
pub fn mcp_server() {
    bridge::mcp::run(
        std::io::stdin().lock(),
        std::io::stdout(),
        bridge::mcp::Env::from_process(),
    );
}

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
        .manage(PrCache::default())
        .setup(move |app| {
            let paths = app.path();
            let config_dir = paths.app_config_dir()?;
            app.manage(Accounts::new(&config_dir, paths.home_dir()?));
            app.manage(Suggestions::new(&paths.app_data_dir()?));
            app.manage(Messages::prepare(&paths.app_data_dir()?));
            // Without its pipe the bridge is off, and every tab starts as it did before.
            let bridge =
                Bridge::start(messages::AppHooks(app.handle().clone())).unwrap_or_else(|e| {
                    eprintln!("messages between accounts are off: {e}");
                    Bridge::off()
                });
            app.manage(bridge);
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
            commands::tab_env,
            commands::tab_set_env,
            commands::tab_move_folder,
            commands::load_profiles,
            commands::reset_profiles,
            commands::detect_accounts,
            commands::suggest_config_dir,
            commands::add_account,
            commands::rename_account,
            commands::remove_account,
            commands::set_default_account,
            commands::set_account_color,
            commands::set_start_folder,
            commands::set_shell,
            commands::set_sidebar_width,
            commands::set_panel,
            commands::set_alerts,
            commands::set_suggestions,
            commands::set_setup_step,
            commands::account_status,
            commands::tree_load,
            commands::tree_save,
            commands::continue_on_account,
            commands::install_cli,
            panel::panel_changes,
            panel::panel_pr,
            panel::panel_open_pr,
            panel::panel_open_folder,
            panel::panel_open_vscode,
            panel::panel_connectors,
            panel::panel_skills,
            messages::set_messages,
            messages::bridge_snapshot,
            messages::bridge_decide,
            messages::bridge_discard,
            messages::bridge_type_in,
            open::launch::open_take,
        ])
        .build(context)
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<Bridge>().shutdown();
            handle.state::<PtyManager>().kill_all();
            handle.state::<Saved>().flush();
            job::spare_children_on_exit();
        }
    });
}
