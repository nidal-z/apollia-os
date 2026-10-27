//! A folder in pause nested in a sound granted folder: whatever path or walk
//! leads there, no file tool reads or writes in it.

use super::*;
use crate::executor::{ToolExecutionError, ToolExecutor};
use crate::tools::{
    file_edit::FileEdit, file_glob::FileGlob, file_grep::FileGrep, file_list::FileList,
    file_read::FileRead, file_write::FileWrite,
};
use serde_json::{json, Value};

/// The secret held by the folder in pause.
const SECRET: &str = "SECRET in the paused folder";

/// A sound folder `outer`, granted to read and to write, holding `o.md`, a
/// folder `inner` in pause holding `n.md`, and a link `outer/link` to
/// `inner`; both stored as folder connections.
struct Nested {
    _dir: tempfile::TempDir,
    outer: PathBuf,
    inner: PathBuf,
    roots: FolderRoots,
}

fn nested() -> Nested {
    let dir = tempfile::tempdir().expect("tempdir");
    let outer = dir.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).expect("mkdir");
    std::fs::write(outer.join("o.md"), "outer note").expect("write");
    std::fs::write(inner.join("n.md"), SECRET).expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&inner, outer.join("link")).expect("symlink");
    let stored = |id: &str, path: &Path| FolderConnection {
        id: id.to_owned(),
        path: path.to_string_lossy().into_owned(),
        label: id.to_owned(),
        added_at: "2026-10-27T00:00:00Z".to_owned(),
    };
    let folders = [stored("outer", &outer), stored("inner", &inner)];
    let access = vec![
        folder_access_name("outer", "file_read"),
        folder_access_name("outer", "file_write"),
        folder_access_name("inner", "file_read"),
    ];
    let paused: HashSet<String> = ["folder:inner".to_owned()].into_iter().collect();
    let (_, roots) = resolve_folder_access(access, &folders, &paused);
    Nested {
        _dir: dir,
        outer,
        inner,
        roots,
    }
}

impl Nested {
    fn reading(&self) -> crate::SandboxSpec {
        self.roots.sandbox(self.roots.read.clone())
    }

    fn writing(&self) -> crate::SandboxSpec {
        self.roots.sandbox(self.roots.write.clone())
    }
}

/// Whether `outcome` is the refusal for the pause.
fn refused_for_pause(outcome: &Result<Value, ToolExecutionError>) -> bool {
    matches!(
        outcome,
        Err(ToolExecutionError::ExecutionFailed { code, message })
            if code == "paused" && message.contains(crate::sandbox_path::PAUSED_REASON)
    )
}

#[tokio::test]
async fn a_relative_path_into_a_nested_folder_in_pause_is_refused_for_the_pause() {
    // GIVEN a sound folder holding a folder in pause
    let nested = nested();
    let read = FileRead::new(nested.reading()).expect("file_read");

    // WHEN a relative path leads into the folder in pause, and another reads
    // a file of the sound folder
    let relative = read.execute(json!({ "path": "inner/n.md" })).await;
    let sound = read.execute(json!({ "path": "o.md" })).await;

    // THEN the first is refused for the pause and reads nothing, the second
    // is served
    assert!(refused_for_pause(&relative), "{relative:?}");
    assert!(sound.expect("o.md").to_string().contains("outer note"));
}

#[tokio::test]
async fn every_spelling_of_a_path_into_the_folder_in_pause_is_refused() {
    // GIVEN the same folders
    let nested = nested();
    let read = FileRead::new(nested.reading()).expect("file_read");
    let list = FileList::new(nested.reading()).expect("file_list");
    let absolute = nested.inner.join("n.md");
    let through_parent = nested.outer.join("x/../inner/n.md");

    // WHEN the folder in pause is reached by an absolute path, by a `..`,
    // through a link, and named with a final slash
    let outcomes = [
        read.execute(json!({ "path": absolute })).await,
        read.execute(json!({ "path": through_parent })).await,
        read.execute(json!({ "path": "./x/../inner/n.md" })).await,
        list.execute(json!({ "dir": "inner/" })).await,
        list.execute(json!({ "dir": format!("{}/", nested.inner.display()) }))
            .await,
        #[cfg(unix)]
        read.execute(json!({ "path": "link/n.md" })).await,
    ];

    // THEN each is refused for the pause
    for outcome in &outcomes {
        assert!(refused_for_pause(outcome), "{outcome:?}");
    }
}

#[tokio::test]
async fn a_path_into_the_folder_in_pause_in_another_case_reads_nothing() {
    // GIVEN the same folders, the name of the folder in pause in capitals
    let nested = nested();
    let read = FileRead::new(nested.reading()).expect("file_read");
    let shouted = nested.outer.join("INNER/n.md");

    // WHEN it is read
    let outcome = read.execute(json!({ "path": shouted })).await;

    // THEN on a disk that ignores case the path is the folder in pause and is
    // refused for it; on one that does not, it names no file. The secret is
    // never read either way.
    let case_blind = nested.outer.join("INNER").exists();
    if case_blind {
        assert!(refused_for_pause(&outcome), "{outcome:?}");
    } else {
        assert!(outcome.is_err(), "{outcome:?}");
    }
}

#[tokio::test]
async fn a_walk_over_the_sound_folder_does_not_enter_the_folder_in_pause() {
    // GIVEN the same folders
    let nested = nested();
    let grep = FileGrep::new(nested.reading()).expect("file_grep");
    let glob = FileGlob::new(nested.reading()).expect("file_glob");
    let list = FileList::new(nested.reading()).expect("file_list");

    // WHEN the sound folder is searched without a path, globbed and listed
    // recursively
    let grepped = grep
        .execute(json!({ "pattern": "SECRET|outer note" }))
        .await
        .expect("grep");
    let globbed = glob
        .execute(json!({ "pattern": "**/*.md" }))
        .await
        .expect("glob");
    let listed = list
        .execute(json!({ "recursive": true }))
        .await
        .expect("list");

    // THEN the file of the sound folder is found, and nothing of the folder
    // in pause is read or named, by its own name or through the link
    let (grepped, globbed, listed) = (grepped.to_string(), globbed.to_string(), listed.to_string());
    assert!(grepped.contains("outer note"), "{grepped}");
    assert!(!grepped.contains("SECRET"), "{grepped}");
    assert!(globbed.contains("o.md"), "{globbed}");
    assert!(!globbed.contains("n.md"), "{globbed}");
    assert!(listed.contains("o.md"), "{listed}");
    assert!(!listed.contains("n.md"), "{listed}");
    assert!(!listed.contains("inner"), "{listed}");
}

#[tokio::test]
async fn nothing_is_written_in_the_folder_in_pause() {
    // GIVEN the same folders, the sound one granted for writing
    let nested = nested();
    let write = FileWrite::new(nested.writing()).expect("file_write");
    let edit = FileEdit::new(nested.writing()).expect("file_edit");

    // WHEN a new file is written in the folder in pause, and its file edited
    let written = write
        .execute(json!({ "path": "inner/new.md", "content": "x" }))
        .await;
    let edited = edit
        .execute(json!({ "path": "inner/n.md", "old_text": "SECRET", "new_text": "gone", "replace_all": false }))
        .await;
    let sound = write
        .execute(json!({ "path": "new.md", "content": "x" }))
        .await;

    // THEN both are refused for the pause and the folder is untouched, while
    // the sound folder takes the write
    assert!(refused_for_pause(&written), "{written:?}");
    assert!(refused_for_pause(&edited), "{edited:?}");
    assert!(!nested.inner.join("new.md").exists());
    assert_eq!(
        std::fs::read_to_string(nested.inner.join("n.md")).expect("n.md"),
        SECRET
    );
    assert!(sound.is_ok(), "{sound:?}");
}

#[tokio::test]
async fn a_sound_folder_nested_in_a_folder_in_pause_is_still_served() {
    // GIVEN the reverse: the outer folder in pause, the inner one sound and
    // granted
    let dir = tempfile::tempdir().expect("tempdir");
    let outer = dir.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).expect("mkdir");
    std::fs::write(inner.join("n.md"), "inner note").expect("write");
    std::fs::write(outer.join("o.md"), SECRET).expect("write");
    let stored = |id: &str, path: &Path| FolderConnection {
        id: id.to_owned(),
        path: path.to_string_lossy().into_owned(),
        label: id.to_owned(),
        added_at: "2026-10-27T00:00:00Z".to_owned(),
    };
    let folders = [stored("outer", &outer), stored("inner", &inner)];
    let paused: HashSet<String> = ["folder:outer".to_owned()].into_iter().collect();
    let (_, roots) = resolve_folder_access(
        vec![folder_access_name("inner", "file_read")],
        &folders,
        &paused,
    );
    let read = FileRead::new(roots.sandbox(roots.read.clone())).expect("file_read");

    // WHEN the inner file and the outer one are read
    let served = read.execute(json!({ "path": "n.md" })).await;
    let outside = read.execute(json!({ "path": outer.join("o.md") })).await;

    // THEN the inner folder is served, and the outer one, no root, is not
    assert!(served.expect("n.md").to_string().contains("inner note"));
    assert!(outside.is_err(), "{outside:?}");
}

#[test]
fn the_folder_a_path_lands_in_is_read_as_the_tools_read_it() {
    // GIVEN the same folders
    let nested = nested();
    let folder = |input: Option<&str>| nested.roots.folder_of(input).map(str::to_owned);
    let absolute = nested.inner.join("n.md").display().to_string();
    let through_parent = nested.outer.join("x/../inner/n.md").display().to_string();

    // WHEN the folder of a relative, an absolute, a `..`, a final slash, a
    // link, a missing path and a sound path is named
    // THEN the relative ones land under the first granted folder, as the
    // tools resolve them, and the deepest folder is named
    assert_eq!(folder(Some("inner/n.md")).as_deref(), Some("folder:inner"));
    assert_eq!(folder(Some(&absolute)).as_deref(), Some("folder:inner"));
    assert_eq!(
        folder(Some(&through_parent)).as_deref(),
        Some("folder:inner")
    );
    assert_eq!(folder(Some("inner/")).as_deref(), Some("folder:inner"));
    #[cfg(unix)]
    assert_eq!(folder(Some("link/n.md")).as_deref(), Some("folder:inner"));
    assert_eq!(folder(None).as_deref(), Some("folder:outer"));
    assert_eq!(folder(Some("o.md")).as_deref(), Some("folder:outer"));
    assert_eq!(folder(Some("/elsewhere/x.md")), None);
}

#[test]
fn a_path_from_home_lands_where_the_tools_resolve_it() {
    // GIVEN the same folders, the directory that holds them standing for the
    // home directory
    let nested = nested();
    let home = nested.outer.parent().expect("base").to_path_buf();
    let anchor = nested.roots.anchor().expect("anchor");

    // WHEN a path spelled from `~` into the folder in pause lands, as the
    // tools land it, and its folder is named
    let landed = crate::sandbox_path::landing_from("~/outer/inner/n.md", anchor, Some(&home))
        .expect("lands");
    let reached = nested.roots.folder_landed(&landed);

    // THEN it is the folder in pause
    assert_eq!(reached, Some("folder:inner"));
}
