//! A walk over a sound folder that holds a folder in pause names nothing it
//! found through that folder, and a folder paused after the tools were built
//! is out from their next call on.

use std::sync::{Arc, RwLock};

use super::*;
use crate::executor::ToolExecutor;
use crate::tools::{
    file_glob::FileGlob, file_grep::FileGrep, file_list::FileList, file_read::FileRead,
};
use serde_json::json;

/// The secret held by the folder in pause.
const SECRET: &str = "SECRETZZ in the paused folder";

/// Pauses the test sets while the tools run.
struct Switch(RwLock<HashSet<String>>);

impl crate::PauseReader for Switch {
    fn paused(&self) -> HashSet<String> {
        self.0.read().expect("pauses").clone()
    }
}

impl Switch {
    fn set(&self, connection: &str, paused: bool) {
        let mut pauses = self.0.write().expect("pauses");
        if paused {
            pauses.insert(connection.to_owned());
        } else {
            pauses.remove(connection);
        }
    }
}

/// A sound folder `outer` holding `o.md`, `sub/s.md` and a folder `inner`
/// that holds `n.md`; a link `inner/up` to `outer/sub`, out of `inner` into
/// the sound folder, a link `outer/link` to `inner`, and a link `outer/away`
/// to a folder outside every root.
struct Tree {
    _dir: tempfile::TempDir,
    inner: PathBuf,
    roots: FolderRoots,
}

fn tree(paused: &HashSet<String>) -> Tree {
    let dir = tempfile::tempdir().expect("tempdir");
    let outer = dir.path().join("outer");
    let inner = outer.join("inner");
    let away = dir.path().join("away");
    std::fs::create_dir_all(&inner).expect("mkdir");
    std::fs::create_dir_all(&away).expect("mkdir");
    std::fs::write(outer.join("o.md"), "outer note").expect("write");
    std::fs::create_dir_all(outer.join("sub")).expect("mkdir");
    std::fs::write(outer.join("sub/s.md"), "sub note").expect("write");
    std::fs::write(inner.join("n.md"), SECRET).expect("write");
    std::fs::write(away.join("a.md"), "away note").expect("write");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outer.join("sub"), inner.join("up")).expect("symlink");
        std::os::unix::fs::symlink(&inner, outer.join("link")).expect("symlink");
        std::os::unix::fs::symlink(&away, outer.join("away")).expect("symlink");
    }
    let stored = |id: &str, path: &Path| FolderConnection {
        id: id.to_owned(),
        path: path.to_string_lossy().into_owned(),
        label: id.to_owned(),
        added_at: "2026-10-27T00:00:00Z".to_owned(),
    };
    let (_, roots) = resolve_folder_access(
        vec![folder_access_name("outer", "file_read")],
        &[stored("outer", &outer), stored("inner", &inner)],
        paused,
    );
    Tree {
        _dir: dir,
        inner,
        roots,
    }
}

fn pause_of_inner() -> HashSet<String> {
    ["folder:inner".to_owned()].into_iter().collect()
}

#[cfg(unix)]
#[tokio::test]
async fn a_walk_names_nothing_it_found_through_the_folder_in_pause() {
    // GIVEN the folder `inner` in pause, a link in it back to the sound
    // folder, and a link from the sound folder into it
    let tree = tree(&pause_of_inner());
    let spec = || tree.roots.sandbox(tree.roots.read.clone());
    let grep = FileGrep::new(spec()).expect("file_grep");
    let glob = FileGlob::new(spec()).expect("file_glob");

    // WHEN the sound folder is searched without a path and globbed
    let grepped = grep
        .execute(json!({ "pattern": "SECRETZZ|outer note|sub note" }))
        .await
        .expect("grep")
        .to_string();
    let globbed = glob
        .execute(json!({ "pattern": "**/*.md" }))
        .await
        .expect("glob")
        .to_string();

    // THEN the files of the sound folder are found by their own paths, and
    // no path named goes through the folder in pause, by its name or by the
    // link into it
    for said in [&grepped, &globbed] {
        assert!(said.contains("\"o.md\""), "{said}");
        assert!(said.contains("\"sub/s.md\""), "{said}");
        assert!(!said.contains("inner/"), "{said}");
        assert!(!said.contains("link/"), "{said}");
        assert!(!said.contains("SECRETZZ"), "{said}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_out_of_every_root_is_listed_and_not_followed() {
    // GIVEN the same tree, nothing in pause
    let tree = tree(&HashSet::new());
    let list = FileList::new(tree.roots.sandbox(tree.roots.read.clone())).expect("file_list");

    // WHEN the sound folder is listed recursively
    let listed = list
        .execute(json!({ "recursive": true }))
        .await
        .expect("list")
        .to_string();

    // THEN the link out is listed by its name, and nothing behind it
    assert!(listed.contains("\"away\""), "{listed}");
    assert!(!listed.contains("a.md"), "{listed}");
}

#[tokio::test]
async fn a_folder_paused_after_the_tools_were_built_is_out_from_their_next_call() {
    // GIVEN the tools of a turn built while nothing was in pause, reading the
    // pauses at each call
    let tree = tree(&HashSet::new());
    let switch = Arc::new(Switch(RwLock::new(HashSet::new())));
    let live = crate::LivePauses::new(switch.clone(), tree.roots.stored.clone());
    let spec = || {
        tree.roots
            .sandbox(tree.roots.read.clone())
            .with_live_pauses(Some(live.clone()))
    };
    let read = FileRead::new(spec()).expect("file_read");
    let grep = FileGrep::new(spec()).expect("file_grep");
    let glob = FileGlob::new(spec()).expect("file_glob");
    let list = FileList::new(spec()).expect("file_list");
    let before = read
        .execute(json!({ "path": "inner/n.md" }))
        .await
        .expect("served before the pause")
        .to_string();

    // WHEN `inner` is paused, then the agent reads, misses, greps, globs and
    // lists in the same turn
    switch.set("folder:inner", true);
    let said = [
        read.execute(json!({ "path": "inner/n.md" })).await,
        read.execute(json!({ "path": "n.md" })).await,
        grep.execute(json!({ "pattern": "SECRETZZ" })).await,
        glob.execute(json!({ "pattern": "**/n.md" })).await,
        list.execute(json!({ "recursive": true })).await,
    ]
    .map(|outcome| match outcome {
        Ok(value) => value.to_string(),
        Err(error) => error.to_string(),
    });

    // THEN it was served before, and nothing of it is read or named after,
    // but the refusal repeating the path asked for
    assert!(before.contains("SECRETZZ"), "{before}");
    assert!(said[0].contains("paused"), "{}", said[0]);
    for said in &said {
        assert!(!said.contains("SECRETZZ"), "{said}");
    }
    for said in &said[1..] {
        assert!(!said.contains("n.md\""), "{said}");
        assert!(!said.contains("inner/"), "{said}");
        assert!(!said.contains(": inner"), "{said}");
    }
    assert!(tree.inner.join("n.md").exists());

    // AND once resumed, it is served again at the next call
    switch.set("folder:inner", false);
    let resumed = read
        .execute(json!({ "path": "inner/n.md" }))
        .await
        .expect("served after the resume");
    assert!(resumed.to_string().contains("SECRETZZ"));
}
