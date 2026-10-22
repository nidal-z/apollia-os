use apollia_connectors::microsoft::onedrive::{DriveItem, FolderFacet};
use tempfile::TempDir;

use super::*;

fn choice(id: &str, name: &str, kind: ChoiceKind) -> OneDriveChoice {
    OneDriveChoice {
        format_version: 1,
        id: id.into(),
        name: name.into(),
        kind,
        path: format!("/drive/root:/{name}"),
        drive_id: None,
    }
}

fn child(id: &str, folder: bool) -> DriveItem {
    serde_json::from_value(serde_json::json!({ "id": id, "name": id }))
        .map(|mut item: DriveItem| {
            if folder {
                item.folder = Some(FolderFacet { child_count: 0 });
            }
            item
        })
        .unwrap()
}

#[test]
fn test_the_choices_are_kept_per_account_and_forgotten_one_by_one() {
    // GIVEN an empty choices file and two Microsoft accounts
    let dir = TempDir::new().unwrap();
    let prefs = prefs_path_in(dir.path());

    // WHEN Ada chooses a folder and a file, chooses the folder again renamed,
    // Bob chooses nothing, and Ada then forgets the file
    record(
        &prefs,
        "ada@example.test",
        choice("f1", "Clients", ChoiceKind::Folder),
    )
    .unwrap();
    record(
        &prefs,
        " ada@example.test ",
        choice("x1", "budget.txt", ChoiceKind::File),
    )
    .unwrap();
    let renamed = record(
        &prefs,
        "ada@example.test",
        choice("f1", "Customers", ChoiceKind::Folder),
    )
    .unwrap();
    let after_forget = forget(&prefs, "ada@example.test", "x1").unwrap();

    // THEN the second choice of the folder replaced the first, Bob has none,
    // and the forgotten file is gone while the folder stays
    assert_eq!(renamed.len(), 2);
    assert_eq!(renamed[0].name, "Customers");
    assert!(renamed[0].is_folder);
    assert!(list(&prefs, "bob@example.test").unwrap().is_empty());
    assert_eq!(after_forget.len(), 1);
    assert_eq!(after_forget[0].id, "f1");
    assert_eq!(
        choices_of(&prefs, "ada@example.test").unwrap()[0].kind,
        ChoiceKind::Folder
    );
}

#[test]
fn test_a_malformed_file_or_an_oversized_input_is_refused() {
    // GIVEN a choices file that does not parse
    let dir = TempDir::new().unwrap();
    let prefs = prefs_path_in(dir.path());
    std::fs::create_dir_all(prefs.parent().unwrap()).unwrap();
    std::fs::write(&prefs, "accounts = 3").unwrap();

    // WHEN it is read, and an account or item no Graph can name is used
    let malformed = choices_of(&prefs, "ada@example.test").unwrap_err();
    let account = list(&prefs, &"a".repeat(400)).unwrap_err();
    let item = forget(&prefs, "ada@example.test", "  ").unwrap_err();

    // THEN each is refused rather than read as nothing chosen
    assert!(matches!(malformed, OneDriveChoiceError::Malformed(_)));
    assert!(matches!(account, OneDriveChoiceError::InvalidAccount));
    assert!(matches!(item, OneDriveChoiceError::InvalidItem));
}

#[test]
fn test_a_browsed_folder_marks_its_chosen_children() {
    // GIVEN a folder with a chosen subfolder and a file not chosen
    let choices = [choice("sub", "Sub", ChoiceKind::Folder)];

    // WHEN its children are turned into entries
    let entries = entries(vec![child("sub", true), child("note.txt", false)], &choices);

    // THEN the subfolder is chosen and a folder, the file neither
    assert!(entries[0].chosen && entries[0].is_folder);
    assert!(!entries[1].chosen && !entries[1].is_folder);
}
