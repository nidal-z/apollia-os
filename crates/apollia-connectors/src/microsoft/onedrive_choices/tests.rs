use serde_json::{json, Value};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::http::HttpClient;
use crate::microsoft::onedrive::{FolderFacet, ItemReference};

const TOKEN: &str = "token-test";

fn refresh() -> impl FnMut() -> std::future::Ready<Result<String, ConnectorError>> + Send {
    || std::future::ready(Ok(TOKEN.to_owned()))
}

fn item(id: &str, name: &str, parent: Option<&str>) -> Value {
    match parent {
        Some(p) => json!({ "id": id, "name": name, "file": { "mimeType": "text/plain" },
                           "parentReference": { "path": p, "driveId": "d1" } }),
        None => json!({ "id": id, "name": name, "file": { "mimeType": "text/plain" } }),
    }
}

fn choices() -> Vec<OneDriveChoice> {
    vec![
        OneDriveChoice {
            format_version: 1,
            id: "fold-clients".into(),
            name: "Clients".into(),
            kind: ChoiceKind::Folder,
            path: "/drive/root:/Clients".into(),
            drive_id: Some("d1".into()),
        },
        OneDriveChoice {
            format_version: 1,
            id: "file-budget".into(),
            name: "budget.txt".into(),
            kind: ChoiceKind::File,
            path: "/drive/root:/Finance/budget.txt".into(),
            drive_id: Some("d1".into()),
        },
    ]
}

async fn answer(server: &MockServer, at: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn client(server: &MockServer) -> OneDriveClient {
    OneDriveClient::with_base_url(HttpClient::new("test").unwrap(), &server.uri())
}

fn placed(id: &str, name: &str, parent: &str, drive: Option<&str>) -> DriveItem {
    DriveItem {
        id: id.into(),
        name: name.into(),
        size: None,
        file: None,
        folder: None,
        web_url: None,
        last_modified_date_time: None,
        parent_reference: Some(ItemReference {
            id: None,
            drive_id: drive.map(str::to_owned),
            path: Some(parent.into()),
        }),
    }
}

#[tokio::test]
async fn test_a_file_outside_the_choices_is_refused_with_a_reason_and_never_read() {
    // GIVEN a fake Graph holding a file under HR, outside every choice, and a
    // file under the chosen Clients folder
    let server = MockServer::start().await;
    answer(
        &server,
        "/items/secret",
        item("secret", "salaries.txt", Some("/drive/root:/HR")),
    )
    .await;
    answer(
        &server,
        "/items/in-1",
        item("in-1", "brief.txt", Some("/drive/root:/Clients/Ada")),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/items/secret/content"))
        .respond_with(ResponseTemplate::new(200).set_body_string("salaries"))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/items/in-1/content"))
        .respond_with(ResponseTemplate::new(200).set_body_string("the brief"))
        .mount(&server)
        .await;
    let client = client(&server);
    let choices = choices();
    let drive = ChosenOneDrive::new(&client, &choices);

    // WHEN an agent downloads both, and reads the metadata of the outside one
    let refused = drive
        .download("secret", TOKEN, &mut refresh())
        .await
        .unwrap_err();
    let metadata = drive
        .get_metadata("secret", TOKEN, &mut refresh())
        .await
        .unwrap_err();
    let read = drive.download("in-1", TOKEN, &mut refresh()).await.unwrap();

    // THEN the outside file is refused with the reason, its content never
    // requested, and the inside file is read
    for error in [refused.to_string(), metadata.to_string()] {
        assert!(
            error.contains("`salaries.txt` is outside the folders and files chosen in Apollia"),
            "{error}"
        );
    }
    assert_eq!(read, b"the brief");
}

#[tokio::test]
async fn test_search_reads_only_inside_the_chosen_folders_and_files() {
    // GIVEN a fake Graph whose drive-wide search answers an outside file, and
    // whose folder search lets one outside item through beside an inside one
    let server = MockServer::start().await;
    let everywhere = json!({ "value": [
        item("in-1", "report.txt", Some("/drive/root:/Clients/Ada")),
        item("out-1", "report.txt", Some("/drive/root:/HR")),
    ]});
    Mock::given(method("GET"))
        .and(path_regex(r"^/root/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(everywhere.clone()))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/items/fold-clients/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(everywhere))
        .mount(&server)
        .await;
    let client = client(&server);
    let choices = choices();
    let drive = ChosenOneDrive::new(&client, &choices);

    // WHEN an agent searches the drive
    let found = drive
        .search("report", 10, TOKEN, &mut refresh())
        .await
        .unwrap();

    // THEN only the item under the chosen folder comes back, and the
    // drive-wide search is never asked
    let ids: Vec<&str> = found.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["in-1"]);
}

#[tokio::test]
async fn test_search_finds_a_chosen_file_by_its_name() {
    // GIVEN a chosen folder whose search finds nothing and a chosen file named budget
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/items/fold-clients/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [] })))
        .mount(&server)
        .await;
    answer(
        &server,
        "/items/file-budget",
        item("file-budget", "budget.txt", Some("/drive/root:/Finance")),
    )
    .await;
    let client = client(&server);
    let choices = choices();
    let drive = ChosenOneDrive::new(&client, &choices);

    // WHEN an agent searches for budget
    let found = drive
        .search("BUDGET", 10, TOKEN, &mut refresh())
        .await
        .unwrap();

    // THEN the chosen file comes back
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "file-budget");
}

#[tokio::test]
async fn test_recent_items_are_kept_only_inside_the_choices() {
    // GIVEN recent items: one inside, one outside, one Graph gave no path for
    // that sits inside, one with no path that Graph no longer finds
    let server = MockServer::start().await;
    answer(
        &server,
        "/recent",
        json!({ "value": [
            item("in-1", "a.txt", Some("/drive/root:/Clients")),
            item("out-1", "b.txt", Some("/drive/root:/Private")),
            item("nopath-in", "c.txt", None),
            item("gone", "d.txt", None),
        ]}),
    )
    .await;
    answer(
        &server,
        "/items/nopath-in",
        item("nopath-in", "c.txt", Some("/drives/d1/root:/Clients/Sub")),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/items/gone"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({ "error": { "code": "itemNotFound" } })),
        )
        .mount(&server)
        .await;
    let client = client(&server);
    let choices = choices();
    let drive = ChosenOneDrive::new(&client, &choices);

    // WHEN an agent lists the recent items
    let recent = drive.list_recent(10, TOKEN, &mut refresh()).await.unwrap();

    // THEN only the two items inside the chosen folder are kept
    let ids: Vec<&str> = recent.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["in-1", "nopath-in"]);
}

#[tokio::test]
async fn test_nothing_chosen_refuses_every_read_without_asking_graph() {
    // GIVEN an account with no choice, and a Graph that would answer anything
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "value": [] })))
        .expect(0)
        .mount(&server)
        .await;
    let client = client(&server);
    let drive = ChosenOneDrive::new(&client, &[]);

    // WHEN an agent searches, lists the recent items and reads an item
    let errors = [
        drive
            .search("x", 10, TOKEN, &mut refresh())
            .await
            .unwrap_err(),
        drive
            .list_recent(10, TOKEN, &mut refresh())
            .await
            .unwrap_err(),
        drive
            .download("x", TOKEN, &mut refresh())
            .await
            .unwrap_err(),
    ];

    // THEN each is refused with the reason
    for error in errors {
        assert!(
            error
                .to_string()
                .contains("no OneDrive folder or file is chosen"),
            "{error}"
        );
    }
}

#[test]
fn test_placement_reads_the_path_without_case_encoding_or_sibling_confusion() {
    // GIVEN the choices, and items placed by Graph in several spellings
    let choices = choices();

    // WHEN each item is placed
    let cases = [
        (
            placed("a", "x.txt", "/drive/root:/clients/Ada", None),
            Placement::Inside,
        ),
        (
            placed("b", "x.txt", "/drives/d1/root:/Clients", Some("d1")),
            Placement::Inside,
        ),
        (
            placed("c", "x.txt", "/drive/root:/My%20Docs", None),
            Placement::Outside,
        ),
        (
            placed("d", "x.txt", "/drive/root:/Clients2", None),
            Placement::Outside,
        ),
        (
            placed("e", "x.txt", "/drives/d2/root:/Clients", Some("d2")),
            Placement::Outside,
        ),
        (
            placed("f", "budget.txt", "/drive/root:/Finance", None),
            Placement::Outside,
        ),
        (
            placed("file-budget", "renamed.txt", "/drive/root:/Elsewhere", None),
            Placement::Inside,
        ),
    ];

    // THEN a different case is the same place, a sibling folder sharing the
    // prefix is outside, another drive is outside, and a chosen file is
    // matched by id, never by name
    for (item, expected) in cases {
        assert_eq!(placement(&choices, &item), expected, "{}", item.id);
    }
    let mut notes = placed("m", "Notes", "/drive/root:/My%20Docs", None);
    notes.folder = Some(FolderFacet { child_count: 1 });
    let encoded = OneDriveChoice::from_item(&notes).unwrap();
    assert_eq!(
        placement(
            &[encoded],
            &placed("n", "x.txt", "/drive/root:/My Docs/Notes", None)
        ),
        Placement::Inside
    );
}

#[test]
fn test_the_root_of_the_drive_cannot_be_chosen() {
    // GIVEN the root of the drive, which Graph answers without a parent path
    let root: DriveItem = serde_json::from_value(
        json!({ "id": "root-id", "name": "root", "folder": { "childCount": 4 } }),
    )
    .unwrap();

    // WHEN it is chosen
    let refused = OneDriveChoice::from_item(&root).unwrap_err();

    // THEN it is refused, since it would open the whole drive
    assert!(
        refused.to_string().contains("root of the drive"),
        "{refused}"
    );
}

#[tokio::test]
async fn test_browsing_then_choosing_a_folder_records_the_place_graph_gives() {
    // GIVEN a fake Graph whose root holds a Clients folder, itself holding a file
    let server = MockServer::start().await;
    let clients = json!({ "id": "fold-c", "name": "Clients", "folder": { "childCount": 1 },
                          "parentReference": { "path": "/drive/root:", "driveId": "d1" } });
    answer(
        &server,
        "/root/children",
        json!({ "value": [clients.clone()] }),
    )
    .await;
    answer(
        &server,
        "/items/fold-c/children",
        json!({ "value": [
            item("brief", "brief.txt", Some("/drive/root:/Clients")),
        ]}),
    )
    .await;
    answer(&server, "/items/fold-c", clients).await;
    let client = client(&server);

    // WHEN the user browses the root, opens the folder, and chooses it
    let root = client
        .list_children(None, 200, TOKEN, refresh())
        .await
        .unwrap();
    let inside = client
        .list_children(Some(&root[0].id), 200, TOKEN, refresh())
        .await
        .unwrap();
    let chosen = client
        .get_metadata("fold-c", TOKEN, refresh())
        .await
        .unwrap();
    let choice = OneDriveChoice::from_item(&chosen).unwrap();

    // THEN the choice is the folder with the path Graph gave, and the file seen
    // while browsing it sits inside it
    assert_eq!(root[0].name, "Clients");
    assert_eq!(choice.kind, ChoiceKind::Folder);
    assert_eq!(choice.path, "/drive/root:/Clients");
    assert_eq!(choice.drive_id.as_deref(), Some("d1"));
    assert_eq!(placement(&[choice], &inside[0]), Placement::Inside);
}
