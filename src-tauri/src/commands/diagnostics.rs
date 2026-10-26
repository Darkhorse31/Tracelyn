use crate::diagnostics::DiagnosticEntry;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn get_server_diagnostics(
    state: State<AppState>,
    infra: State<crate::infra::InfraState>,
    server_id: String,
) -> Vec<DiagnosticEntry> {
    let mut rows = state.diagnostics_for_server(&server_id);
    rows.extend(infra.diagnostics(&server_id));
    rows
}
