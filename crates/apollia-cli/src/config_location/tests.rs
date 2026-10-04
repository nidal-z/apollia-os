use super::*;

const PACKAGE_MANIFEST: &str = "[package]\nid = \"demo\"\nversion = \"0.1.0\"\n\n[[tools]]\nid = \"ping\"\nmodule = \"tools/ping.py\"\n";

fn write(path: &Path, body: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

#[test]
fn an_explicit_path_wins_verbatim() {
    // GIVEN a path named on the command line
    let explicit = Path::new("/somewhere/else/custom.toml");
    // WHEN the configuration file is resolved
    let resolved = resolve(Some(explicit));
    // THEN it is that path, with no search of its own
    assert_eq!(resolved, explicit.to_path_buf());
}

#[test]
fn files_at_the_three_former_locations_are_all_named() {
    // GIVEN a file in the working directory, under $XDG_CONFIG_HOME and under
    // ~/.config, and none at the canonical location
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    write(&cwd.path().join("apollia.toml"), "[api]\nport = 7000\n");
    write(&xdg.path().join("apollia").join("apollia.toml"), "[api]\n");
    write(
        &home
            .path()
            .join(".config")
            .join("apollia")
            .join("apollia.toml"),
        "[api]\n",
    );
    // WHEN the former locations are probed
    let found = legacy_files(home.path(), Some(cwd.path()), Some(xdg.path()));
    // THEN each of the three is named, once
    assert_eq!(found.len(), 3, "{found:?}");
}

#[test]
fn a_package_manifest_in_the_working_directory_is_not_a_former_config() {
    // GIVEN a working directory holding the manifest of an agent package
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    write(&cwd.path().join("apollia.toml"), PACKAGE_MANIFEST);
    // WHEN the former locations are probed
    let found = legacy_files(home.path(), Some(cwd.path()), None);
    // THEN the manifest is not named, so nobody is told to move it
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_canonical_file_is_never_a_former_location() {
    // GIVEN a working directory that is the data directory itself
    let home = tempfile::tempdir().unwrap();
    let canonical = paths::config_file_under(home.path());
    write(&canonical, "[api]\n");
    let data_dir = canonical.parent().unwrap().to_path_buf();
    // WHEN the former locations are probed from there
    let found = legacy_files(home.path(), Some(&data_dir), None);
    // THEN the canonical file is not reported as misplaced
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_move_command_names_both_paths() {
    // GIVEN a former location and the canonical one
    let from = Path::new("/w/apollia.toml");
    let to = Path::new("/h/.apollia/apollia.toml");
    // WHEN the command to move it is written
    let cmd = move_command(from, to);
    // THEN it carries both paths, source first
    let a = cmd.find("/w/apollia.toml").unwrap();
    let b = cmd.find("/h/.apollia/apollia.toml").unwrap();
    assert!(a < b, "{cmd}");
}
