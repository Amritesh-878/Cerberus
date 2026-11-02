use tauri::{Builder, Runtime};

/// Cmd+C, Cmd+V and Cmd+Q reach a macOS webview only through a menu; Tauri's own binds Cmd+W to Close Window.
#[cfg(target_os = "macos")]
pub fn with_app_menu<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder.menu(app_menu)
}

#[cfg(not(target_os = "macos"))]
pub fn with_app_menu<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
}

#[cfg(target_os = "macos")]
fn app_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, PredefinedMenuItem as Item, Submenu};
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
    Menu::with_items(app, &[&app_items, &edit, &window])
}
