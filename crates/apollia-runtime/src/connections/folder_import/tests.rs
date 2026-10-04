//! Tests of the one-time import of the folders the former settings named.

use super::*;

fn write_config(dir: &Path, content: &str) -> PathBuf {
    let path = dir.join("apollia.toml");
    std::fs::write(&path, content).expect("write apollia.toml");
    path
}

fn stored_paths(data_dir: &Path) -> Vec<String> {
    crate::api::routes_connections::folders::store(data_dir)
        .expect("open system.db")
        .list()
        .expect("list folders")
        .into_iter()
        .map(|f| f.path)
        .collect()
}

fn canonical(path: &Path) -> String {
    path.canonicalize()
        .expect("canonical")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn every_former_folder_but_the_home_folder_becomes_a_connection_once() {
    // GIVEN a file naming the home folder, two folders as trusted paths and a
    // third as the default workspace
    let data = tempfile::tempdir().expect("tempdir");
    let a = tempfile::tempdir().expect("tempdir");
    let b = tempfile::tempdir().expect("tempdir");
    let work = tempfile::tempdir().expect("tempdir");
    let config = write_config(
        data.path(),
        &format!(
            "[chat]\ndefault_workspace = {:?}\n\n[filesystem]\ntrusted_paths = [\"~\", {:?}, {:?}]\n",
            work.path(),
            a.path(),
            b.path()
        ),
    );
    let governance = ConnectionGovernance::without_policy(None);

    // WHEN the runtime starts
    let added = import_legacy_folders(Some(&config), data.path(), &governance);

    // THEN the three folders are connections, in the order written, and the
    // home folder is not one
    assert_eq!(added, 3);
    assert_eq!(
        stored_paths(data.path()),
        vec![
            canonical(a.path()),
            canonical(b.path()),
            canonical(work.path())
        ]
    );

    // AND WHEN the user removes one and the runtime starts again
    let store = crate::api::routes_connections::folders::store(data.path()).expect("store");
    let first = store.list().expect("list").remove(0);
    store.remove(&first.id).expect("remove");
    let again = import_legacy_folders(Some(&config), data.path(), &governance);

    // THEN nothing is imported again: the removed folder stays removed
    assert_eq!(again, 0);
    assert_eq!(stored_paths(data.path()).len(), 2);
}

#[test]
fn a_missing_folder_or_the_data_directory_is_skipped() {
    // GIVEN a file naming a folder that no longer exists and the data
    // directory itself
    let data = tempfile::tempdir().expect("tempdir");
    let config = write_config(
        data.path(),
        &format!(
            "[filesystem]\ntrusted_paths = [\"/no/such/folder/anywhere\", {:?}]\n",
            data.path()
        ),
    );
    let governance = ConnectionGovernance::without_policy(None);

    // WHEN the runtime starts
    let added = import_legacy_folders(Some(&config), data.path(), &governance);

    // THEN nothing is added, and the start goes on
    assert_eq!(added, 0);
    assert!(stored_paths(data.path()).is_empty());
}

#[test]
fn the_home_folder_is_left_out_however_it_is_written() {
    // GIVEN a home folder written as `~`, `~/` and an absolute path
    let home = PathBuf::from("/Users/ada");
    let content = "[filesystem]\ntrusted_paths = [\"~\", \"~/\", \"/Users/ada\", \"~/Projects\"]\n";

    // WHEN the former folders are read against that home
    let paths = folder_paths_under(content, Some(&home));

    // THEN only the folder under it remains
    assert_eq!(paths, vec![home.join("Projects")]);
}
