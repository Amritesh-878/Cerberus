//! The app's side of services: the shell choice in, the list and output out, and the commands.

use std::ffi::OsStr;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::accounts::Accounts;
use crate::blocking::blocking;
use crate::programs::{Finder, ProgramChoice, pick_shell};
use crate::services::store::{Draft, ServiceId, StoreError};
use crate::services::{Services, Sink, View, WatchId};

pub struct AppSink(pub AppHandle);

impl Sink for AppSink {
    fn changed(&self, view: View) {
        // Fails only once the window is gone, and then nobody is left to show it.
        let _ = self.0.emit("services", view);
    }

    fn shell(&self, wanted: Option<&str>) -> Option<ProgramChoice> {
        let setting = self.0.state::<Accounts>().load().ok().and_then(|p| p.shell);
        let choices = Finder::from_env().choices();
        let installed = |id: &str| {
            choices
                .iter()
                .any(|c| c.is_shell && c.program.is_some() && c.id == id)
        };
        let id = wanted.filter(|id| installed(id)).or(setting.as_deref());
        pick_shell(choices.clone(), id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("it has no address yet")]
    NoUrl,
    #[error("could not open it: {0}")]
    Open(std::io::Error),
}

impl serde::Serialize for ServiceError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command(async)]
pub fn services_view(services: State<'_, Services>) -> View {
    services.view()
}

#[tauri::command]
pub async fn service_add(app: AppHandle, draft: Draft) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.add(&draft)
    })
    .await
}

#[tauri::command]
pub async fn service_update(
    app: AppHandle,
    id: ServiceId,
    draft: Draft,
) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.update(id, &draft)
    })
    .await
}

#[tauri::command]
pub async fn service_remove(app: AppHandle, id: ServiceId) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.remove(id)
    })
    .await
}

#[tauri::command]
pub async fn service_start(app: AppHandle, id: ServiceId) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.start(id)
    })
    .await
}

#[tauri::command]
pub async fn service_stop(app: AppHandle, id: ServiceId) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.stop(id)
    })
    .await
}

#[tauri::command]
pub async fn service_restart(app: AppHandle, id: ServiceId) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.restart(id)
    })
    .await
}

/// The command, for the edit form; nothing else asks for it.
#[tauri::command]
pub async fn service_command(app: AppHandle, id: ServiceId) -> Result<String, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.command(id)
    })
    .await
}

/// Opens only an address the service printed, never one the webview names.
#[tauri::command]
pub async fn service_open_url(app: AppHandle, id: ServiceId) -> Result<(), ServiceError> {
    blocking(move || {
        let services = app.state::<Services>();
        let url = services.url(id).ok_or(ServiceError::NoUrl)?;
        crate::panel::reveal::open(OsStr::new(&url)).map_err(ServiceError::Open)
    })
    .await
}

#[tauri::command]
pub async fn services_fold(app: AppHandle, folded: bool) -> Result<View, StoreError> {
    blocking(move || {
        let services = app.state::<Services>();
        services.fold(folded)
    })
    .await
}

/// The output so far, then each new chunk, on `output` as raw bytes; `None` when the service is gone.
#[tauri::command]
pub async fn service_watch(
    app: AppHandle,
    id: ServiceId,
    output: Channel<InvokeResponseBody>,
) -> Option<WatchId> {
    blocking(move || {
        let services = app.state::<Services>();
        services.watch(id, move |bytes| {
            // A send fails only once the viewer is gone, and its unwatch follows.
            let _ = output.send(InvokeResponseBody::Raw(bytes.to_vec()));
        })
    })
    .await
}

#[tauri::command(async)]
pub fn service_unwatch(services: State<'_, Services>, watch: WatchId) {
    services.unwatch(watch);
}
