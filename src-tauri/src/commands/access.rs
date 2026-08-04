use crate::saas_access::NativeAccountAccessStatus;

fn worker_error(operation: &str, error: impl std::fmt::Display) -> String {
    format!("Recorder account-access {operation} worker failed: {error}")
}

#[tauri::command]
pub async fn get_account_access_status() -> Result<NativeAccountAccessStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::saas_access::status)
        .await
        .map_err(|error| worker_error("status", error))?
}

#[tauri::command]
pub async fn refresh_account_access() -> Result<NativeAccountAccessStatus, String> {
    crate::saas_access::refresh().await
}
