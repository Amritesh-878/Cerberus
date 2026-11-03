use tauri::{Builder, Runtime};

/// Cmd+C, Cmd+V and Cmd+Q reach a macOS webview only through a menu, and Tauri's own binds Cmd+W to Close Window.
#[cfg(target_os = "macos")]
pub fn with_app_menu<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder.menu(app_menu).on_menu_event(|app, event| {
        if event.id() == SHORTCUTS {
            use tauri::Emitter;
            // Fails only once the window is gone, and then there is no sheet to show.
            let _ = app.emit("show-shortcuts", ());
        }
    })
}

/// The menu item's id; its Cmd+/ is the same chord the webview answers to elsewhere.
#[cfg(target_os = "macos")]
const SHORTCUTS: &str = "shortcuts";

#[cfg(not(target_os = "macos"))]
pub fn with_app_menu<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
}

#[cfg(target_os = "macos")]
fn app_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem as Item, Submenu};
    let name = app.package_info().name.clone();
    let app_items = Submenu::with_items(
        app,
        name,
        true,
        &[
            &Item::about(app, None, None)?,
            &Item::separator(app)?,
            &Item::hide(app, None)?,
            &Item::hide_others(app, None)?,
            &Item::separator(app)?,
            &Item::quit(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &Item::undo(app, None)?,
            &Item::redo(app, None)?,
            &Item::separator(app)?,
            &Item::cut(app, None)?,
            &Item::copy(app, None)?,
            &Item::paste(app, None)?,
            &Item::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[&Item::minimize(app, None)?, &Item::maximize(app, None)?],
    )?;
    let shortcuts = MenuItem::with_id(app, SHORTCUTS, "Keyboard Shortcuts", true, Some("Cmd+/"))?;
    let help = Submenu::with_items(app, "Help", true, &[&shortcuts])?;
    Menu::with_items(app, &[&app_items, &edit, &window, &help])
}
