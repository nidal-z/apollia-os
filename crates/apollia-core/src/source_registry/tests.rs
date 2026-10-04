use super::*;

fn store() -> (tempfile::TempDir, SourceRegistryStore) {
    let dir = tempfile::TempDir::new().unwrap();
    let store = SourceRegistryStore::open(&dir.path().join("system.db")).unwrap();
    (dir, store)
}

fn folder(label: &str, reference: Option<&str>) -> NewSource {
    NewSource {
        kind: SourceKind::Folder,
        label: label.to_owned(),
        group: None,
        reference: reference.map(str::to_owned),
    }
}

#[test]
fn test_a_source_added_is_read_back_then_removed() {
    // GIVEN an empty registry
    let (_dir, store) = store();
    assert!(store.list().unwrap().is_empty());

    // WHEN a folder source is added, read, listed, then removed
    let added = store
        .create(&NewSource {
            kind: SourceKind::Folder,
            label: "  Contracts ".to_owned(),
            group: Some("Office".to_owned()),
            reference: Some("conn-contracts".to_owned()),
        })
        .unwrap();
    let read = store.get(&added.id).unwrap();
    let listed = store.list().unwrap();
    let removed = store.delete(&added.id).unwrap();

    // THEN it is stored trimmed, unindexed, dated, and gone after removal
    assert!(added.id.starts_with("src-"), "{}", added.id);
    assert_eq!(added.kind, SourceKind::Folder);
    assert_eq!(added.label, "Contracts");
    assert_eq!(added.group.as_deref(), Some("Office"));
    assert_eq!(added.reference.as_deref(), Some("conn-contracts"));
    assert_eq!(added.index_state, IndexState::Absent);
    assert!(added.added_at.ends_with('Z'), "{}", added.added_at);
    assert_eq!(read.as_ref(), Some(&added));
    assert_eq!(listed, vec![added.clone()]);
    assert_eq!(removed, added);
    assert!(store.get(&added.id).unwrap().is_none());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn test_a_folder_a_file_a_mailbox_or_a_code_folder_without_a_reference_is_refused() {
    // GIVEN an empty registry
    let (_dir, store) = store();

    // WHEN each kind is added without a reference, or with a blank one
    let refused: Vec<_> = [
        SourceKind::Folder,
        SourceKind::File,
        SourceKind::Mail,
        SourceKind::CodeFolder,
    ]
    .into_iter()
    .flat_map(|kind| {
        [None, Some("   ")].map(|reference| {
            store.create(&NewSource {
                kind,
                label: "x".to_owned(),
                group: None,
                reference: reference.map(str::to_owned),
            })
        })
    })
    .collect();
    let accepted: Vec<_> = [SourceKind::Profile, SourceKind::Memory]
        .into_iter()
        .map(|kind| {
            store.create(&NewSource {
                kind,
                label: kind.as_str().to_owned(),
                group: None,
                reference: None,
            })
        })
        .collect();

    // THEN the four kinds that point at something are refused by a typed
    // error, nothing is stored for them, and the others are accepted
    assert_eq!(refused.len(), 8);
    for result in &refused {
        assert!(
            matches!(result, Err(SourceRegistryError::MissingReference(kind)) if kind.requires_reference()),
            "{result:?}"
        );
    }
    assert!(accepted.iter().all(Result::is_ok), "{accepted:?}");
    assert_eq!(store.list().unwrap().len(), 2);
}

#[test]
fn test_a_blank_label_and_an_unknown_identifier_are_refused() {
    // GIVEN an empty registry
    let (_dir, store) = store();

    // WHEN a source without a label is added and an absent one is removed
    let unlabeled = store.create(&folder("  ", Some("conn")));
    let absent = store.delete("src-absent");

    // THEN both are refused by their typed error
    assert!(matches!(unlabeled, Err(SourceRegistryError::EmptyLabel)));
    assert!(matches!(absent, Err(SourceRegistryError::NotFound(id)) if id == "src-absent"));
}

#[test]
fn test_the_kinds_and_states_round_trip_and_refuse_other_tokens() {
    // GIVEN every kind and state token
    // WHEN each is parsed back from its stored form, and two foreign tokens are parsed
    let kinds: Vec<_> = SourceKind::ALL
        .iter()
        .map(|k| SourceKind::parse(k.as_str()).unwrap())
        .collect();
    let states: Vec<_> = IndexState::ALL
        .iter()
        .map(|s| IndexState::parse(s.as_str()).unwrap())
        .collect();
    let dossier = SourceKind::parse("dossier");
    let done = IndexState::parse("done");

    // THEN the round trip is exact and the foreign tokens are refused
    assert_eq!(kinds, SourceKind::ALL.to_vec());
    assert_eq!(states, IndexState::ALL.to_vec());
    assert!(matches!(dossier, Err(SourceRegistryError::UnknownKind(_))));
    assert!(matches!(
        done,
        Err(SourceRegistryError::UnknownIndexState(_))
    ));
}

#[test]
fn test_a_source_list_names_registry_sources_profile_or_memory() {
    // GIVEN a registry holding one folder
    let (_dir, store) = store();
    let contracts = store.create(&folder("Contracts", Some("conn"))).unwrap();

    // WHEN lists are checked: known entries, an unknown name, a kind that needs an identifier
    let known = store.check_entries(&[
        contracts.id.clone(),
        "profile".to_owned(),
        "memory".to_owned(),
    ]);
    let unknown = store.check_entries(&["profile".to_owned(), "documents".to_owned()]);
    let bare_kind = store.check_entries(&["folder".to_owned()]);

    // THEN only identifiers of the registry and the two token kinds pass
    assert!(known.is_ok(), "{known:?}");
    assert!(matches!(unknown, Err(SourceRegistryError::UnknownSource(e)) if e == "documents"));
    assert!(matches!(bare_kind, Err(SourceRegistryError::UnknownSource(e)) if e == "folder"));
}

#[test]
fn test_a_v7_file_gains_the_sources_table_and_keeps_its_rows() {
    // GIVEN a system.db written at version seven, with a default embeddings model
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("system.db");
    {
        let conn = Connection::open(&path).unwrap();
        for step in &crate::system_db::SYSTEM_DB_MIGRATIONS[..7] {
            step(&conn).unwrap();
        }
        conn.pragma_update(None, "user_version", 7).unwrap();
        conn.execute(
            "INSERT INTO embedding_default (id, model_id) VALUES (1, 'multilingual-e5-small')",
            [],
        )
        .unwrap();
    }

    // WHEN the registry opens it
    let store = SourceRegistryStore::open(&path).unwrap();

    // THEN the file is at the current version, the table answers, and the row is kept
    assert!(store.list().unwrap().is_empty());
    let version: i64 = store
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        version,
        i64::from(crate::system_db::SYSTEM_DB_SCHEMA_VERSION)
    );
    let model: String = store
        .conn
        .query_row("SELECT model_id FROM embedding_default", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(model, "multilingual-e5-small");
}

#[test]
fn test_the_table_itself_refuses_a_folder_without_reference() {
    // GIVEN a registry
    let (_dir, store) = store();

    // WHEN a row is written past the store, without a reference
    let written = store.conn.execute(
        "INSERT INTO sources (id, kind, label) VALUES ('src-x', 'mail', 'Inbox')",
        [],
    );

    // THEN the CHECK constraint refuses it
    assert!(written.is_err());
}

#[test]
fn test_the_code_folder_is_offered_on_a_folder_only_in_developer_mode() {
    // GIVEN the developer mode off, then on
    let modes = [false, true];

    // WHEN the kinds offered on a folder connection are listed in each mode
    let [off, on] = modes.map(SourceKind::offered_on_folder);

    // THEN the code folder is offered only when the mode is on, the indexed
    // folder in both
    assert_eq!(off, vec![SourceKind::Folder]);
    assert_eq!(on, vec![SourceKind::Folder, SourceKind::CodeFolder]);
}

#[test]
fn test_a_code_folder_is_accepted_whatever_the_mode_once_it_names_a_connection() {
    // GIVEN an empty registry; the registry knows nothing of the developer mode
    let (_dir, store) = store();

    // WHEN a code folder is added on a folder connection
    let added = store
        .create(&NewSource {
            kind: SourceKind::CodeFolder,
            label: "apollia".to_owned(),
            group: None,
            reference: Some("fc-1".to_owned()),
        })
        .unwrap();

    // THEN it is stored with the connection it points at
    assert_eq!(added.kind, SourceKind::CodeFolder);
    assert_eq!(added.reference.as_deref(), Some("fc-1"));
}

#[test]
fn test_the_index_state_of_a_source_is_recorded_and_an_absent_source_refused() {
    // GIVEN a registry holding one folder source, its index absent
    let (_dir, store) = store();
    let added = store.create(&folder("Contracts", Some("fc-1"))).unwrap();

    // WHEN its index state is set to indexing, then ready, and the state of
    // a source that is not there is set
    store
        .set_index_state(&added.id, IndexState::Indexing)
        .unwrap();
    let during = store.get(&added.id).unwrap().unwrap().index_state;
    store.set_index_state(&added.id, IndexState::Ready).unwrap();
    let after = store.get(&added.id).unwrap().unwrap().index_state;
    let absent = store.set_index_state("src-absent", IndexState::Ready);

    // THEN each state is read back, and the absent source is refused
    assert_eq!(during, IndexState::Indexing);
    assert_eq!(after, IndexState::Ready);
    assert!(matches!(absent, Err(SourceRegistryError::NotFound(_))));
}
