use super::*;

#[test]
fn test_a_name_that_could_leave_its_folder_is_refused() {
    // GIVEN plain names and names that are paths
    let plain = ["veille", "web-search_worker.v2"];
    let paths = ["", ".", "..", "a/b", "a\\b", &"x".repeat(MAX_NAME_LEN + 1)];

    // WHEN each is checked
    let accepted: Vec<bool> = plain.iter().map(|n| check_name(n).is_ok()).collect();
    let refused: Vec<bool> = paths.iter().map(|n| check_name(n).is_err()).collect();

    // THEN only the plain names pass
    assert!(accepted.iter().all(|ok| *ok));
    assert!(refused.iter().all(|err| *err), "{refused:?}");
}
