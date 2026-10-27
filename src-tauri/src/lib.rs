mod commands;
mod programs;
mod pty;

use tauri::webview::PageLoadEvent;
use tauri::{Manager, RunEvent};

use crate::pty::PtyManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(PtyManager::default())
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<PtyManager>().kill_all();
        }
    });
}
