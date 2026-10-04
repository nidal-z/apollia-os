use super::*;

/// A folder holding an `APOLLIA.md` and one source file, outside any git
/// repository the test runs in.
fn code_folder(label: &str) -> (tempfile::TempDir, CodeFolder) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("APOLLIA.md"), "Never commit on main.").unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src").join("lib.rs"), "pub fn f() {}").unwrap();
    let folder = CodeFolder {
        label: label.to_owned(),
        path: dir.path().to_path_buf(),
    };
    (dir, folder)
}

fn names(tools: &[&str]) -> Vec<String> {
    tools.iter().map(|t| (*t).to_owned()).collect()
}

#[tokio::test]
async fn test_the_rules_and_the_tree_of_a_code_folder_are_read_on_demand() {
    // GIVEN the tool over one code folder holding rules and a source file
    let (_dir, folder) = code_folder("apollia");
    let tool = CodeContext::new(vec![folder]);

    // WHEN the agent calls it with no argument
    let out = tool.execute(json!({})).await.unwrap();

    // THEN the folder is answered with its rules and its tree, read now
    let folders = out["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["label"], "apollia");
    let sections = folders[0]["sections"].as_array().unwrap();
    let parts: Vec<&str> = sections
        .iter()
        .map(|s| s["part"].as_str().unwrap())
        .collect();
    assert!(parts.contains(&"rules"), "{out}");
    assert!(parts.contains(&"tree"), "{out}");
    let rules = sections.iter().find(|s| s["part"] == "rules").unwrap();
    assert!(rules["content"]
        .as_str()
        .unwrap()
        .contains("Never commit on main."));
}

#[tokio::test]
async fn test_a_call_reads_only_the_parts_and_the_folder_it_names() {
    // GIVEN the tool over two code folders
    let (_a, first) = code_folder("first");
    let (_b, second) = code_folder("second");
    let tool = CodeContext::new(vec![first, second]);

    // WHEN the agent asks for the tree of the second one only
    let out = tool
        .execute(json!({ "folder": "second", "parts": ["tree"] }))
        .await
        .unwrap();

    // THEN only that folder and that part are answered
    let folders = out["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["label"], "second");
    let sections = folders[0]["sections"].as_array().unwrap();
    assert!(!sections.is_empty(), "{out}");
    assert!(sections.iter().all(|s| s["part"] == "tree"), "{out}");
}

#[tokio::test]
async fn test_a_folder_not_checked_and_an_unknown_part_are_refused() {
    // GIVEN the tool over one code folder
    let (_dir, folder) = code_folder("apollia");
    let tool = CodeContext::new(vec![folder]);

    // WHEN the agent names another folder, then an unknown part
    let other = tool.execute(json!({ "folder": "/etc" })).await;
    let part = tool.execute(json!({ "parts": ["secrets"] })).await;

    // THEN both are refused, the first naming the folders it may read
    match other {
        Err(ToolExecutionError::ExecutionFailed { code, message }) => {
            assert_eq!(code, "unknown_folder");
            assert!(message.contains("apollia"), "{message}");
        }
        other => panic!("expected unknown_folder, got {other:?}"),
    }
    assert!(matches!(part, Err(ToolExecutionError::InvalidInput { .. })));
}

#[test]
fn test_the_tool_belongs_to_the_reading_family() {
    // GIVEN the descriptor of the tool
    let descriptor = CodeContext::descriptor();

    // WHEN its family and its validity are read
    let valid = descriptor.validate();

    // THEN it reads, without approval, and the registry accepts it
    assert_eq!(descriptor.name, CODE_CONTEXT_TOOL);
    assert_eq!(descriptor.family, ActionFamily::Read);
    assert!(descriptor.is_read_only);
    assert!(valid.is_ok(), "{valid:?}");
}

#[test]
fn test_the_tool_is_granted_by_a_code_folder_and_only_by_it() {
    // GIVEN an access naming the tool, and one that does not
    let naming = names(&["file_read", "code_context"]);
    let silent = names(&["file_read"]);

    // WHEN each is read without a code folder, then with one
    let naming_without = granted_with_code_folders(naming.clone(), false);
    let silent_without = granted_with_code_folders(silent.clone(), false);
    let naming_with = granted_with_code_folders(naming, true);
    let silent_with = granted_with_code_folders(silent, true);

    // THEN the tool is there exactly when a code folder is checked, once
    assert_eq!(naming_without, names(&["file_read"]));
    assert_eq!(silent_without, names(&["file_read"]));
    assert_eq!(naming_with, names(&["file_read", "code_context"]));
    assert_eq!(silent_with, names(&["file_read", "code_context"]));
}
