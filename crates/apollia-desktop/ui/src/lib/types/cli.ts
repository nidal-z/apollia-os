// ─── CLI install ───

/** CLI install status (Docker Desktop model). */
export interface CliStatus {
  bundled: boolean;
  bundled_path: string | null;
  installed: boolean;
  symlink_path: string;
  version: string;
  needs_privilege: boolean;
}
