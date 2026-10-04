use super::*;

fn folder(id: &str, path: &str) -> FolderConnection {
    FolderConnection {
        id: id.to_owned(),
        path: path.to_owned(),
        label: id.to_owned(),
        added_at: "2026-10-04T00:00:00Z".to_owned(),
    }
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn the_roots_are_the_union_of_the_granted_folders_in_the_order_added() {
    // GIVEN three stored folders, the first two granted, one of them for writing
    let folders = [
        folder("a", "/data/a"),
        folder("b", "/data/b"),
        folder("c", "/data/c"),
    ];
    let access = names(&[
        "conn:folder:b/file_read",
        "conn:folder:b/file_write",
        "conn:folder:a/file_list",
        "web_search",
    ]);

    // WHEN the access is resolved
    let (tools, roots) = resolve_folder_access(access, &folders);

    // THEN both granted folders are read roots, oldest first, the folder
    // granted for writing alone is a write root, and the ungranted one is none
    assert_eq!(
        roots.read,
        vec![PathBuf::from("/data/a"), PathBuf::from("/data/b")]
    );
    assert_eq!(roots.write, vec![PathBuf::from("/data/b")]);
    assert_eq!(roots.anchor(), Some(Path::new("/data/a")));
    // AND the executor checks the natives, the other accesses untouched
    assert_eq!(
        tools,
        names(&["file_read", "file_write", "file_list", "web_search"])
    );
}

#[test]
fn no_folder_granted_means_no_root() {
    // GIVEN a stored folder and an agent granted the bare file tools only
    let folders = [folder("a", "/data/a")];
    let access = names(&["file_read", "file_write"]);

    // WHEN the access is resolved
    let (tools, roots) = resolve_folder_access(access, &folders);

    // THEN the tools stay granted but reach nothing
    assert_eq!(tools, names(&["file_read", "file_write"]));
    assert!(roots.read.is_empty() && roots.write.is_empty());
    assert_eq!(roots.anchor(), None);
}

#[test]
fn an_access_to_a_removed_folder_grants_nothing() {
    // GIVEN an access naming a folder that is no longer stored, and a
    // malformed folder access
    let folders = [folder("a", "/data/a")];
    let access = names(&["conn:folder:gone/file_read", "conn:folder:a/bash_executor"]);

    // WHEN the access is resolved
    let (tools, roots) = resolve_folder_access(access, &folders);

    // THEN neither becomes a tool nor a root
    assert!(tools.is_empty(), "tools were {tools:?}");
    assert!(roots.read.is_empty());
}

#[test]
fn a_folder_access_reads_back_into_its_parts() {
    // GIVEN the access to the read tool of a folder
    let name = folder_access_name("4f2c", "file_read");

    // WHEN it is parsed, along with names that are not folder accesses
    let parsed = parse_folder_access(&name);

    // THEN the folder and the tool come back, and the others are refused
    assert_eq!(name, "conn:folder:4f2c/file_read");
    assert_eq!(parsed, Some(("4f2c", "file_read")));
    assert_eq!(parse_folder_access("conn:folder:/file_read"), None);
    assert_eq!(parse_folder_access("conn:google:a@b.c/gmail.send"), None);
    assert_eq!(parse_folder_access("file_read"), None);
}

#[test]
fn the_granted_folders_are_read_from_system_db() {
    // GIVEN a data directory whose system.db holds one folder
    let data = tempfile::tempdir().expect("tempdir");
    let target = tempfile::tempdir().expect("tempdir");
    let store =
        FolderConnectionStore::open(&DataFile::System.path(data.path())).expect("open system.db");
    let stored = store
        .add(&target.path().to_string_lossy(), None)
        .expect("add the folder");

    // WHEN an agent granted its read tool is resolved
    let (tools, roots) = granted_folders(
        vec![folder_access_name(&stored.id, "file_grep")],
        data.path(),
    );

    // THEN the folder is its only root, and file_grep its tool
    assert_eq!(roots.read, vec![PathBuf::from(&stored.path)]);
    assert!(roots.write.is_empty());
    assert_eq!(tools, names(&["file_grep"]));
}

#[test]
fn a_data_directory_without_system_db_grants_no_folder() {
    // GIVEN an empty data directory
    let data = tempfile::tempdir().expect("tempdir");

    // WHEN a folder access is resolved against it
    let (tools, roots) = granted_folders(names(&["conn:folder:a/file_read"]), data.path());

    // THEN nothing is granted and no file is created
    assert!(tools.is_empty());
    assert!(roots.read.is_empty());
    assert!(!DataFile::System.path(data.path()).exists());
}
