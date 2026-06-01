//! E2E test - all 15 frontend routes have non-crashing backing API endpoints.

/// Mapping of a frontend route to its primary backing API endpoint.
struct RouteProbe {
    /// Svelte frontend route name, used in failure messages.
    name: &'static str,
    /// Runtime API endpoint called on page mount.
    api_path: &'static str,
}

/// The 15 frontend routes and the API endpoint each calls on mount.
///
/// Only 5xx server errors (excluding 503 which signals an optional subsystem)
/// count as failures - they indicate the route would render a crash screen.
/// 4xx responses and 503 are acceptable steady states.
const ROUTE_PROBES: &[RouteProbe] = &[
    RouteProbe {
        name: "/",
        api_path: "/api/v1/health",
    },
    RouteProbe {
        name: "/agents",
        api_path: "/api/v1/agents",
    },
    RouteProbe {
        name: "/chat",
        api_path: "/api/v1/sessions",
    },
    RouteProbe {
        name: "/tasks",
        api_path: "/api/v1/tasks",
    },
    RouteProbe {
        name: "/memory",
        api_path: "/api/v1/user/memory",
    },
    RouteProbe {
        name: "/tools",
        api_path: "/api/v1/tools",
    },
    RouteProbe {
        name: "/triggers",
        api_path: "/api/v1/triggers",
    },
    RouteProbe {
        name: "/pipelines",
        api_path: "/api/v1/pipelines",
    },
    RouteProbe {
        name: "/notifications",
        api_path: "/api/v1/notifications/channels",
    },
    RouteProbe {
        name: "/mcp",
        api_path: "/api/v1/mcp/servers",
    },
    RouteProbe {
        name: "/stt",
        api_path: "/api/v1/stt/status",
    },
    RouteProbe {
        name: "/observability",
        api_path: "/api/v1/audit",
    },
    RouteProbe {
        name: "/config",
        api_path: "/api/v1/user/profile",
    },
    RouteProbe {
        name: "/onboarding",
        api_path: "/api/v1/user/memory?category=context",
    },
    RouteProbe {
        name: "/hitl",
        api_path: "/api/v1/approvals/pending",
    },
];

/// Verifies that each of the 15 frontend routes has a healthy backing API
/// endpoint: no connection errors and no 5xx responses (except 503 which
/// indicates an optional subsystem that is simply not configured).
///
/// All 15 routes are probed before failures are reported so that a single
/// run surfaces all broken routes rather than stopping at the first.
#[tokio::test]
#[ignore = "E2E test - requires running runtime and desktop app"]
async fn test_navigation_all_15_routes_no_crash() {
    assert_eq!(
        ROUTE_PROBES.len(),
        15,
        "ROUTE_PROBES must contain exactly 15 entries"
    );

    super::with_retry(|| async {
        let client = super::http_client()?;
        let mut failures: Vec<String> = Vec::new();

        for probe in ROUTE_PROBES {
            let response = client
                .get(super::api_url(probe.api_path))
                .send()
                .await
                .map_err(|e| {
                    format!(
                        "route '{}': connection to {} failed: {e}",
                        probe.name, probe.api_path
                    )
                })?;

            let status = response.status();
            // 503 = optional subsystem not configured - acceptable.
            if status.is_server_error() && status.as_u16() != 503 {
                failures.push(format!(
                    "route '{}' → {} returned HTTP {}",
                    probe.name,
                    probe.api_path,
                    status.as_u16()
                ));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; ").into())
        }
    })
    .await;
}
