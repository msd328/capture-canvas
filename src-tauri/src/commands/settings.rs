use crate::{recording::types::RecorderSettings, state::AppState};
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
    *state.settings.settings.write() = settings.clone();
    state.settings.persist()?;
    Ok(settings)
}
