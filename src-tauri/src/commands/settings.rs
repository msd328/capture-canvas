use crate::{recording::types::RecorderSettings, security, state::AppState};
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> RecorderSettings {
    state.settings.settings.read().clone()
}

#[tauri::command]
pub fn update_settings(
    settings: RecorderSettings,
    state: State<'_, AppState>,
) -> Result<RecorderSettings, String> {
    let settings = security::validate_settings(settings)?;
    *state.settings.settings.write() = settings.clone();
    state.settings.persist()?;
    Ok(settings)
}
