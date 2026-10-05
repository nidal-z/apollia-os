//! The journal has no erase function on the API: no route under
//! `/api/v1/audit` answers `DELETE`.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;
use utoipa::OpenApi as _;

use crate::api::openapi::ApiDoc;
use crate::api::server::APIServer;
use crate::api::test_support::{app_state, StemLoader};

#[tokio::test]
async fn test_no_audit_route_answers_delete() {
    // GIVEN every route the API documents under /api/v1/audit
    let doc = ApiDoc::openapi();
    let audit: Vec<_> = doc
        .paths
        .paths
        .iter()
        .filter(|(path, _)| path.starts_with("/api/v1/audit"))
        .collect();
    let paths: Vec<&str> = audit.iter().map(|(path, _)| path.as_str()).collect();
    for expected in [
        "/api/v1/audit",
        "/api/v1/audit/journal",
        "/api/v1/audit/export",
        "/api/v1/audit/verify",
        "/api/v1/audit/anchor",
    ] {
        assert!(
            paths.contains(&expected),
            "{expected} missing from {paths:?}"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    for (path, item) in audit {
        // WHEN its verbs are listed
        // THEN none of them is DELETE
        assert!(item.delete.is_none(), "{path} documents a DELETE");

        // AND WHEN a DELETE is sent to it on the real router
        let state = app_state(
            Arc::new(StemLoader { refusal: None }),
            dir.path().to_path_buf(),
        );
        let uri: String = path
            .split('/')
            .map(|segment| {
                if segment.starts_with('{') {
                    "x"
                } else {
                    segment
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(&uri)
            .body(Body::empty())
            .unwrap();
        let response = APIServer::build_router_for_test(state)
            .oneshot(request)
            .await
            .unwrap();

        // THEN the router knows the path and refuses the verb
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "DELETE {uri}"
        );
    }
}
