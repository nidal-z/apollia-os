/**
 * Typed Tauri IPC wrapper for the report of the migration of the former
 * projects into sources, read by the notice shown once after it.
 */
import { invoke } from "@tauri-apps/api/core";
import type { MigrationReport } from "$lib/projectMigration/notice";

/** The report of the last run, `null` when no migration has run. */
export function getProjectMigrationReport(): Promise<MigrationReport | null> {
  return invoke<MigrationReport | null>("get_project_migration_report");
}
