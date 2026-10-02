/**
 * Action families of a tool, mirroring `ActionFamily` and `FamilySource` in
 * `crates/apollia-tools/src/descriptor.rs`. The wire identifiers are the
 * serde names of the Rust enums.
 */

/** What a tool does to the user's data or machine. */
export type ActionFamily =
  | "read"
  | "read_personal"
  | "write_or_delete"
  | "send_external"
  | "execute_code"
  /** Nobody classified the tool yet. Treated as sensitive. */
  | "unclassified"
  /** Outside the families: acts on the conversation only (`ask_user`). */
  | "exempt";

/** Where a descriptor's family comes from. */
export type FamilySource = "server" | "selection" | "none" | "builtin" | "package";

/**
 * Whether a family is sensitive: every family other than reading, and the
 * unclassified state. A family this build does not know is sensitive too.
 */
export function isSensitiveFamily(family: ActionFamily | string): boolean {
  return family !== "read" && family !== "exempt";
}
