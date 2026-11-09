//! The app's side of services: the shell choice in, the list and output out, and the commands.

use std::ffi::OsStr;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::accounts::Accounts;
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

#[tauri::command(async)]
pub fn service_add(services: State<'_, Services>, draft: Draft) -> Result<View, StoreError> {
    services.add(&draft)
}

#[tauri::command(async)]
pub fn service_update(
    services: State<'_, Services>,
    id: ServiceId,
    draft: Draft,
) -> Result<View, StoreError> {
    services.update(id, &draft)
}

#[tauri::command(async)]
pub fn service_remove(services: State<'_, Services>, id: ServiceId) -> Result<View, StoreError> {
    services.remove(id)
}

#[tauri::command(async)]
pub fn service_start(services: State<'_, Services>, id: ServiceId) -> Result<View, StoreError> {
    services.start(id)
}

#[tauri::command(async)]
pub fn service_stop(services: State<'_, Services>, id: ServiceId) -> Result<View, StoreError> {
    services.stop(id)
}

#[tauri::command(async)]
pub fn service_restart(services: State<'_, Services>, id: ServiceId) -> Result<View, StoreError> {
    services.restart(id)
}

/// The command, for the edit form; nothing else asks for it.
#[tauri::command(async)]
pub fn service_command(services: State<'_, Services>, id: ServiceId) -> Result<String, StoreError> {
    services.command(id)
}

/// Opens only an address the service printed, never one the webview names.
#[tauri::command(async)]
pub fn service_open_url(services: State<'_, Services>, id: ServiceId) -> Result<(), ServiceError> {
    let url = services.url(id).ok_or(ServiceError::NoUrl)?;
    crate::panel::reveal::open(OsStr::new(&url)).map_err(ServiceError::Open)
}

#[tauri::command(async)]
pub fn services_fold(services: State<'_, Services>, folded: bool) -> Result<View, StoreError> {
    services.fold(folded)
}

/// The output so far, then each new chunk, on `output` as raw bytes; `None` when the service is gone.
#[tauri::command(async)]
pub fn service_watch(
    services: State<'_, Services>,
    id: ServiceId,
    output: Channel<InvokeResponseBody>,
) -> Option<WatchId> {
    services.watch(id, move |bytes| {
        // A send fails only once the viewer is gone, and its unwatch follows.
        let _ = output.send(InvokeResponseBody::Raw(bytes.to_vec()));
    })
}

#[tauri::command(async)]
pub fn service_unwatch(services: State<'_, Services>, watch: WatchId) {
    services.unwatch(watch);
}
