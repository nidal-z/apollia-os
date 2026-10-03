use rusqlite::Connection;

use super::*;
use crate::paths::DataFile;
use crate::system_db::{migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5};

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn open_repo(dir: &tempfile::TempDir) -> ServerTokenRepository {
    ServerTokenRepository::open(&dir.path().join(DataFile::System.file_name())).unwrap()
}

fn columns(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA table_info(server_tokens)").unwrap();
    let names = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
    names.collect::<Result<_, _>>().unwrap()
}

#[test]
fn test_a_v4_system_db_gains_the_token_table_and_keeps_its_rows() {
    // GIVEN a system.db at version 4, holding a backend
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(DataFile::System.file_name());
    {
        let conn = Connection::open(&path).unwrap();
        for migrate in [migrate_v1, migrate_v2, migrate_v3, migrate_v4] {
            migrate(&conn).unwrap();
        }
        conn.pragma_update(None, "user_version", 4).unwrap();
        conn.execute(
            "INSERT INTO llm_backends (name, provider, model) VALUES ('local', 'llama-cpp', '/m.gguf')",
            [],
        )
        .unwrap();
    }

    // WHEN the token store opens it
    drop(open_repo(&dir));

    // THEN the table holds the six columns, the backend survives, and the
    // file is at version 5
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        columns(&conn),
        [
            "id",
            "name",
            "hash",
            "last_four",
            "created_at",
            "last_used_at",
            "revoked_at"
        ]
    );
    let backends: i64 = conn
        .query_row("SELECT COUNT(*) FROM llm_backends", [], |r| r.get(0))
        .unwrap();
    assert_eq!(backends, 1);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 5);
}

#[test]
fn test_replaying_the_token_migration_changes_nothing() {
    // GIVEN a migrated file holding one token
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    repo.insert("editor", HASH_A, "1a2b").unwrap();

    // WHEN the step runs again
    let conn = Connection::open(dir.path().join(DataFile::System.file_name())).unwrap();
    migrate_v5(&conn).unwrap();

    // THEN the token is still there, alone
    assert_eq!(repo.list().unwrap().len(), 1);
}

#[test]
fn test_a_created_token_lists_without_any_use_nor_revocation() {
    // GIVEN an empty store
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);

    // WHEN a token is recorded
    let row = repo.insert("  editor  ", HASH_A, "1a2b").unwrap();

    // THEN it is named, trimmed, dated, never used, in use, and counted
    assert_eq!(row.name, "editor");
    assert_eq!(row.last_four, "1a2b");
    assert!(!row.created_at.is_empty());
    assert_eq!(row.last_used_at, None);
    assert_eq!(row.revoked_at, None);
    assert_eq!(repo.list().unwrap(), vec![row.clone()]);
    assert_eq!(repo.active_count().unwrap(), 1);
    assert_eq!(
        repo.active_hashes().unwrap(),
        vec![(row.id, HASH_A.to_owned())]
    );
}

#[test]
fn test_two_tokens_in_use_never_share_a_name_and_a_revoked_one_frees_it() {
    // GIVEN a token in use named editor
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    repo.insert("editor", HASH_A, "1a2b").unwrap();

    // WHEN a second token takes the name, then the first is revoked by name and
    // the second tries again
    let refused = repo.insert("editor", HASH_B, "3c4d");
    let revoked = repo
        .revoke(&TokenSelector::Name("editor".to_owned()))
        .unwrap();
    let second = repo.insert("editor", HASH_B, "3c4d").unwrap();

    // THEN the first attempt is refused, the revoked row stays listed, and only
    // the second token is in use
    assert!(matches!(refused, Err(ServerTokenError::NameTaken { .. })));
    assert!(revoked.revoked_at.is_some());
    assert_eq!(repo.list().unwrap().len(), 2);
    assert_eq!(repo.active_count().unwrap(), 1);
    assert_eq!(
        repo.active_hashes().unwrap(),
        vec![(second.id, HASH_B.to_owned())]
    );
}

#[test]
fn test_a_token_is_revoked_by_its_identifier_and_an_unknown_one_is_not_found() {
    // GIVEN one token
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    let row = repo.insert("editor", HASH_A, "1a2b").unwrap();

    // WHEN it is revoked by identifier, twice, and an unknown one is revoked
    let first = repo.revoke(&TokenSelector::Id(row.id)).unwrap();
    let again = repo.revoke(&TokenSelector::Id(row.id)).unwrap();
    let unknown = repo.revoke(&TokenSelector::Name("nobody".to_owned()));

    // THEN the revocation is dated once, and the unknown one is not found
    assert!(first.revoked_at.is_some());
    assert_eq!(first.revoked_at, again.revoked_at);
    assert_eq!(repo.active_count().unwrap(), 0);
    assert!(matches!(unknown, Err(ServerTokenError::NotFound { .. })));
}

#[test]
fn test_a_use_is_stamped_on_the_token() {
    // GIVEN a token never used
    let dir = tempfile::TempDir::new().unwrap();
    let repo = open_repo(&dir);
    let row = repo.insert("editor", HASH_A, "1a2b").unwrap();

    // WHEN its use is recorded
    repo.touch(row.id).unwrap();

    // THEN the list shows when
    assert!(repo.list().unwrap()[0].last_used_at.is_some());
}

#[test]
fn test_an_empty_a_numeric_or_an_overlong_name_is_refused() {
    // GIVEN three names the store cannot keep
    let overlong = "x".repeat(MAX_SERVER_TOKEN_NAME_CHARS + 1);
    let names = ["   ", "42", overlong.as_str()];

    // WHEN each is checked
    let results = names.map(validate_token_name);

    // THEN each is refused
    for result in results {
        assert!(
            matches!(result, Err(ServerTokenError::InvalidName { .. })),
            "{result:?}"
        );
    }
}

#[test]
fn test_digits_select_an_identifier_and_anything_else_a_name() {
    // GIVEN a path segment of digits and one of letters
    // WHEN each is parsed
    let id = TokenSelector::parse("12");
    let name = TokenSelector::parse("editor");

    // THEN the first names an identifier, the second a name
    assert_eq!(id, TokenSelector::Id(12));
    assert_eq!(name, TokenSelector::Name("editor".to_owned()));
}
