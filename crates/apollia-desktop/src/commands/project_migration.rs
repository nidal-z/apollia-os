//! The report of the migration of the former projects into sources, read by
//! the notice the interface shows once after it.

use serde_json::Value;
use tauri::State;

use crate::attached::RuntimeLink;

/// Route of the report, served by the runtime in both modes.
const REPORT_ROUTE: &str = "/api/v1/sources/project-migration";

/// The report of the last run of the migration, `None` when no migration
/// has run: nothing to tell the user.
#[tauri::command]
pub async fn get_project_migration_report(
    state: State<'_, RuntimeLink>,
) -> Result<Option<Value>, String> {
    state.get_json_or_none(REPORT_ROUTE).await
}
