//! The named profiles of `apollia-os profile`: `list`, `show [<id>]`,
//! `create`, `rename`, `set <id> <field> <value>`, `set-default` and `delete`.
//!
//! A profile is addressed by its identifier, or by `default` for the default
//! profile. Like the rest of the noun, these verbs open `user_memory.db`
//! directly and need no running runtime.

use std::path::Path;

use apollia_memory::profile_schema::is_canonical;
use apollia_memory::user_memory::{Profile, UserMemoryError, UserMemoryRepository, WrittenBy};

use super::{format_written_by, open_repo};
use crate::exit_codes;

/// The identifier that names the default profile whatever its identifier.
pub(super) const DEFAULT_ALIAS: &str = "default";

fn fail(json: bool, message: String) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &message)
}

fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

/// Why a profile could not be acted on, in the words the user reads.
fn describe(err: &UserMemoryError, id: &str) -> String {
    match err {
        UserMemoryError::NotFound(_) => {
            format!("no profile has the identifier '{id}' (see `apollia-os profile list`)")
        }
        other => other.to_string(),
    }
}

/// The profile `id` names, `default` naming the default profile.
fn find(repo: &UserMemoryRepository, id: &str) -> Result<Profile, String> {
    let found = if id == DEFAULT_ALIAS {
        repo.default_profile().map(Some)
    } else {
        repo.profile(id)
    };
    match found {
        Ok(Some(profile)) => Ok(profile),
        Ok(None) => Err(describe(&UserMemoryError::NotFound(id.to_owned()), id)),
        Err(e) => Err(describe(&e, id)),
    }
}

fn default_mark(profile: &Profile) -> &'static str {
    if profile.is_default {
        "yes"
    } else {
        ""
    }
}

/// `apollia-os profile list`.
pub(super) fn run_list(db_path: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let profiles = match repo.list_profiles() {
        Ok(p) => p,
        Err(e) => return fail(json, format!("read failed: {e}")),
    };
    if json {
        print_json(&profiles);
        return exit_codes::SUCCESS;
    }
    let nw = profiles
        .iter()
        .map(|p| p.name.chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    println!("  {:<36}  {:<nw$}  DEFAULT", "ID", "NAME");
    for p in &profiles {
        println!("  {:<36}  {:<nw$}  {}", p.id, p.name, default_mark(p));
    }
    exit_codes::SUCCESS
}

/// `apollia-os profile show [<id>]`: the profile, its filled fields, then the
/// free-form entries every profile shares.
pub(super) fn run_show(db_path: Option<&Path>, id: Option<&str>, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let profile = match find(&repo, id.unwrap_or(DEFAULT_ALIAS)) {
        Ok(p) => p,
        Err(msg) => return fail(json, msg),
    };
    let entries = match repo.list_all_in(&profile) {
        Ok(v) => v,
        Err(e) => return fail(json, format!("read failed: {e}")),
    };
    if json {
        print_json(&serde_json::json!({ "profile": profile, "entries": entries }));
        return exit_codes::SUCCESS;
    }
    let default = if profile.is_default { ", default" } else { "" };
    println!("  {} ({}{default})", profile.name, profile.id);
    if entries.is_empty() {
        crate::note!("  (profile is empty - run `apollia-os onboard` to fill it)");
        return exit_codes::SUCCESS;
    }
    let kw = entries.iter().map(|e| e.key.len()).max().unwrap_or(8);
    println!("  {:<width$}  WRITTEN BY  VALUE", "KEY", width = kw);
    for e in &entries {
        let by = format_written_by(&e.written_by);
        println!("  {:<width$}  {:<10}  {}", e.key, by, e.value, width = kw);
    }
    exit_codes::SUCCESS
}

/// `apollia-os profile create <name>`.
pub(super) fn run_create(db_path: Option<&Path>, name: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.create_profile(name) {
        Ok(profile) => {
            if json {
                print_json(&profile);
            } else {
                println!("  * created {} ({})", profile.name, profile.id);
            }
            exit_codes::SUCCESS
        }
        Err(UserMemoryError::InvalidKey(_)) => fail(json, "a profile name cannot be empty".into()),
        Err(e) => fail(json, format!("create failed: {e}")),
    }
}

/// `apollia-os profile rename <id> <name>`.
pub(super) fn run_rename(db_path: Option<&Path>, id: &str, name: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let profile = match find(&repo, id) {
        Ok(p) => p,
        Err(msg) => return fail(json, msg),
    };
    match repo.rename_profile(&profile.id, name) {
        Ok(()) => reread(&repo, &profile.id, json, |p| {
            format!("  * renamed {} to {}", p.id, p.name)
        }),
        Err(UserMemoryError::InvalidKey(_)) => fail(json, "a profile name cannot be empty".into()),
        Err(e) => fail(json, describe(&e, id)),
    }
}

/// `apollia-os profile set <id> <field> <value>`: a field of the schema is
/// written on that profile; any other key is a free-form entry, shared by
/// every profile. An empty value empties a field.
pub(super) fn run_set(
    db_path: Option<&Path>,
    id: &str,
    field: &str,
    value: &str,
    json: bool,
) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let profile = match find(&repo, id) {
        Ok(p) => p,
        Err(msg) => return fail(json, msg),
    };
    let written = if is_canonical(field) {
        repo.set_profile_field(&profile.id, field, Some(value))
    } else {
        repo.set_in(&profile, field, value, WrittenBy::User)
    };
    match written {
        Ok(()) => {
            if json {
                print_json(&serde_json::json!({
                    "profile_id": profile.id,
                    "key": field,
                    "value": value,
                    "in_schema": is_canonical(field),
                    "written_by": "user",
                }));
            } else {
                println!("  * {} : {field} = {value}", profile.name);
            }
            exit_codes::SUCCESS
        }
        Err(UserMemoryError::InvalidKey(k)) => fail(
            json,
            format!("'{k}' cannot be written: the key is empty or reserved"),
        ),
        Err(e) => fail(json, format!("set failed: {e}")),
    }
}

/// `apollia-os profile set-default <id>`.
pub(super) fn run_set_default(db_path: Option<&Path>, id: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let profile = match find(&repo, id) {
        Ok(p) => p,
        Err(msg) => return fail(json, msg),
    };
    match repo.set_default_profile(&profile.id) {
        Ok(()) => reread(&repo, &profile.id, json, |p| {
            format!("  * {} ({}) is now the default profile", p.name, p.id)
        }),
        Err(e) => fail(json, describe(&e, id)),
    }
}

/// `apollia-os profile delete <id> --confirm`. The default profile is refused:
/// another one has to be made the default first.
pub(super) fn run_delete(db_path: Option<&Path>, id: &str, confirm: bool, json: bool) -> i32 {
    let Some(repo) = open_repo(db_path, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let profile = match find(&repo, id) {
        Ok(p) => p,
        Err(msg) => return fail(json, msg),
    };
    if profile.is_default {
        return fail(
            json,
            UserMemoryError::DefaultProfile(profile.name.clone()).to_string(),
        );
    }
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("delete the profile '{}'", profile.name),
    ) {
        return code;
    }
    match repo.delete_profile(&profile.id) {
        Ok(()) => {
            if json {
                print_json(&serde_json::json!({ "deleted": profile.id, "name": profile.name }));
            } else {
                println!("  * deleted {} ({})", profile.name, profile.id);
            }
            exit_codes::SUCCESS
        }
        Err(e) => fail(json, describe(&e, id)),
    }
}

/// Reads `id` back after a change and prints it.
fn reread(
    repo: &UserMemoryRepository,
    id: &str,
    json: bool,
    human: impl Fn(&Profile) -> String,
) -> i32 {
    match repo.profile(id) {
        Ok(Some(profile)) => {
            if json {
                print_json(&profile);
            } else {
                println!("{}", human(&profile));
            }
            exit_codes::SUCCESS
        }
        Ok(None) => fail(json, describe(&UserMemoryError::NotFound(id.into()), id)),
        Err(e) => fail(json, format!("read failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("um.db");
        (dir, path)
    }

    fn repo(path: &Path) -> UserMemoryRepository {
        UserMemoryRepository::new(path).expect("opens")
    }

    #[test]
    fn deleting_the_default_profile_is_refused_even_with_confirm() {
        // GIVEN a database whose only profile is the default
        let (_dir, path) = db();
        let default = repo(&path).default_profile().expect("default");

        // WHEN it is deleted by identifier, then by alias, with confirmation
        let by_id = run_delete(Some(&path), &default.id, true, true);
        let by_alias = run_delete(Some(&path), DEFAULT_ALIAS, true, true);

        // THEN both are refused with the general error code, and the profile remains
        assert_eq!(by_id, exit_codes::GENERAL_ERROR);
        assert_eq!(by_alias, exit_codes::GENERAL_ERROR);
        assert_eq!(repo(&path).list_profiles().expect("list").len(), 1);
    }

    #[test]
    fn create_set_set_default_then_delete_the_former_default() {
        // GIVEN a fresh database
        let (_dir, path) = db();
        let first = repo(&path).default_profile().expect("default");

        // WHEN a profile is created, filled, made default, and the former default deleted
        assert_eq!(
            run_create(Some(&path), "Formateur", true),
            exit_codes::SUCCESS
        );
        let created = repo(&path)
            .list_profiles()
            .expect("list")
            .into_iter()
            .find(|p| p.name == "Formateur")
            .expect("created");
        let set = run_set(Some(&path), &created.id, "role", "Formateur", true);
        let made_default = run_set_default(Some(&path), &created.id, true);
        let deleted = run_delete(Some(&path), &first.id, true, true);

        // THEN every step succeeds and the created profile is the only, default, one
        assert_eq!(
            (set, made_default, deleted),
            (
                exit_codes::SUCCESS,
                exit_codes::SUCCESS,
                exit_codes::SUCCESS
            )
        );
        let left = repo(&path).list_profiles().expect("list");
        assert_eq!(left.len(), 1);
        assert!(left[0].is_default);
        assert_eq!(left[0].role.as_deref(), Some("Formateur"));
    }

    #[test]
    fn an_unknown_identifier_is_a_general_error_everywhere() {
        // GIVEN a fresh database
        let (_dir, path) = db();

        // WHEN each verb names an identifier no profile has
        let codes = [
            run_show(Some(&path), Some("missing"), true),
            run_rename(Some(&path), "missing", "x", true),
            run_set(Some(&path), "missing", "role", "x", true),
            run_set_default(Some(&path), "missing", true),
            run_delete(Some(&path), "missing", true, true),
        ];

        // THEN each exits 1
        assert!(codes.iter().all(|c| *c == exit_codes::GENERAL_ERROR));
    }

    #[test]
    fn delete_without_confirm_outside_a_terminal_is_refused() {
        // GIVEN a second profile
        let (_dir, path) = db();
        let other = repo(&path).create_profile("Formateur").expect("created");

        // WHEN it is deleted without --confirm under --json
        let code = run_delete(Some(&path), &other.id, false, true);

        // THEN the deletion is refused and the profile remains
        assert_eq!(code, exit_codes::GENERAL_ERROR);
        assert!(repo(&path).profile(&other.id).expect("read").is_some());
    }
}
