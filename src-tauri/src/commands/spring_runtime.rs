use crate::state::AppState;
use ide_core::{IdeError, IdeResult, RequestId};
use ide_http::spring_runtime::SpringRuntimeSnapshot;
use tauri::{State, Window};

/// Local-only, ephemeral, sanitized Actuator snapshot; never recorded in HTTP history.
#[tauri::command]
#[specta::specta]
pub async fn spring_runtime_snapshot(
    window: Window,
    state: State<'_, AppState>,
    request: RequestId,
    base: String,
    user: String,
    password: String,
) -> IdeResult<SpringRuntimeSnapshot> {
    if state.remote_dev.get(window.label()).is_some() {
        return Err(IdeError::unavailable("Spring Runtime requires a local project"));
    }
    state.project(window.label())?;
    let cancel = state.begin_request(request);
    let result = ide_http::spring_runtime::snapshot(&base, &user, &password, &cancel).await;
    state.end_request(request);
    result
}
