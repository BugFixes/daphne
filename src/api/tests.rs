use axum::http::{HeaderMap, HeaderValue};
use serde_json::json;

use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    AuthenticatedStacktraceEventPayload, GoBugPayload, GoLogPayload, LogEventPayload, Membership,
    Organization, OrganizationAccess, OrganizationRole, Permission, Severity,
    StacktraceEventPayload,
};

use super::{
    AgentAuth, check_org_permission, decode_go_bytes, ensure_user_can_access_clerk_org,
    extract_current_clerk_user_id, extract_required_clerk_org_id, first_meaningful_line,
    format_location, map_go_bug_payload, map_go_log_payload, severity_to_tone,
};

mod rbac_integration {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serial_test::serial;
    use tower::ServiceExt;

    use crate::domain::{
        AddOrganizationMemberRequest, CreateOrganizationRequest, OrganizationRole,
    };
    use crate::test_support::{build_test_app, reset_database};

    async fn seed_org_with_member(
        repository: &crate::repository::Repository,
        clerk_org_id: &str,
        owner_clerk_user_id: &str,
        member_clerk_user_id: &str,
        member_role: OrganizationRole,
    ) -> uuid::Uuid {
        let org = repository
            .create_organization(CreateOrganizationRequest {
                name: "Test Org".to_string(),
                clerk_org_id: Some(clerk_org_id.to_string()),
                owner_clerk_user_id: owner_clerk_user_id.to_string(),
                owner_name: "Owner".to_string(),
            })
            .await
            .expect("org");
        repository
            .add_organization_member(
                org.organization.id,
                owner_clerk_user_id,
                AddOrganizationMemberRequest {
                    clerk_user_id: member_clerk_user_id.to_string(),
                    name: "Member".to_string(),
                    role: member_role,
                },
            )
            .await
            .expect("member");
        org.organization.id
    }

    fn create_account_body() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "name": "Test Account",
            "create_tickets": false,
            "ticket_provider": "github",
            "notification_provider": "slack",
            "ai_enabled": false,
            "use_managed_ai": false,
            "notify_min_level": "warn",
            "rapid_occurrence_window_minutes": 30,
            "rapid_occurrence_threshold": 3
        }))
        .expect("json")
    }

    fn create_agent_body() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "account_id": uuid::Uuid::new_v4(),
            "name": "Test Agent"
        }))
        .expect("json")
    }

    fn add_member_body() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "clerk_user_id": "user_new_member",
            "name": "New Member",
            "role": "member"
        }))
        .expect("json")
    }

    #[tokio::test]
    #[serial]
    async fn member_cannot_create_account() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        seed_org_with_member(
            &repository,
            "org_rbac_acct",
            "user_owner_acct",
            "user_member_acct",
            OrganizationRole::Member,
        )
        .await;

        let request = Request::builder()
            .method("POST")
            .uri("/v1/accounts")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_member_acct")
            .header("X-Clerk-Org-Id", "org_rbac_acct")
            .body(Body::from(create_account_body()))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn admin_can_create_account() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        seed_org_with_member(
            &repository,
            "org_rbac_acct_admin",
            "user_owner_acct_admin",
            "user_admin_acct",
            OrganizationRole::Admin,
        )
        .await;

        let request = Request::builder()
            .method("POST")
            .uri("/v1/accounts")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_owner_acct_admin")
            .header("X-Clerk-Org-Id", "org_rbac_acct_admin")
            .body(Body::from(create_account_body()))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[serial]
    async fn member_cannot_create_agent() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        seed_org_with_member(
            &repository,
            "org_rbac_agent",
            "user_owner_agent",
            "user_member_agent",
            OrganizationRole::Member,
        )
        .await;

        let request = Request::builder()
            .method("POST")
            .uri("/v1/agents")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_member_agent")
            .header("X-Clerk-Org-Id", "org_rbac_agent")
            .body(Body::from(create_agent_body()))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn admin_can_attempt_create_agent() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        seed_org_with_member(
            &repository,
            "org_rbac_agent_admin",
            "user_owner_agent_admin",
            "user_admin_agent",
            OrganizationRole::Admin,
        )
        .await;

        let request = Request::builder()
            .method("POST")
            .uri("/v1/agents")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_owner_agent_admin")
            .header("X-Clerk-Org-Id", "org_rbac_agent_admin")
            .body(Body::from(create_agent_body()))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_ne!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn member_cannot_add_organization_member() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let org_id = seed_org_with_member(
            &repository,
            "org_rbac_membership",
            "user_owner_membership",
            "user_member_membership",
            OrganizationRole::Member,
        )
        .await;

        let request = Request::builder()
            .method("POST")
            .uri(format!("/v1/organizations/{org_id}/memberships"))
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_member_membership")
            .header("X-Clerk-Org-Id", "org_rbac_membership")
            .body(Body::from(add_member_body()))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn member_can_read_bugs() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        seed_org_with_member(
            &repository,
            "org_rbac_bugs",
            "user_owner_bugs",
            "user_member_bugs",
            OrganizationRole::Member,
        )
        .await;

        let request = Request::builder()
            .method("GET")
            .uri("/api/dashboard/bugs")
            .header("X-Clerk-User-Id", "user_member_bugs")
            .header("X-Clerk-Org-Id", "org_rbac_bugs")
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[serial]
    async fn non_member_cannot_read_bugs() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        repository
            .create_organization(CreateOrganizationRequest {
                name: "Other Org".to_string(),
                clerk_org_id: Some("org_rbac_bugs_other".to_string()),
                owner_clerk_user_id: "user_owner_bugs_other".to_string(),
                owner_name: "Owner".to_string(),
            })
            .await
            .expect("org");

        let request = Request::builder()
            .method("GET")
            .uri("/api/dashboard/bugs")
            .header("X-Clerk-User-Id", "user_outsider")
            .header("X-Clerk-Org-Id", "org_rbac_bugs_other")
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

mod role_management_integration {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serial_test::serial;
    use tower::ServiceExt;

    use crate::domain::{
        AddOrganizationMemberRequest, CreateOrganizationRequest, OrganizationRole,
    };
    use crate::test_support::{build_test_app, reset_database};

    async fn seed_org(
        repository: &crate::repository::Repository,
        clerk_org_id: &str,
        owner_clerk_user_id: &str,
    ) -> (uuid::Uuid, uuid::Uuid) {
        let org = repository
            .create_organization(CreateOrganizationRequest {
                name: "Test Org".to_string(),
                clerk_org_id: Some(clerk_org_id.to_string()),
                owner_clerk_user_id: owner_clerk_user_id.to_string(),
                owner_name: "Owner".to_string(),
            })
            .await
            .expect("org");
        let owner_memberships = repository
            .list_organization_memberships(org.organization.id, owner_clerk_user_id)
            .await
            .expect("memberships");
        let owner_user_id = owner_memberships[0].membership.user_id;
        (org.organization.id, owner_user_id)
    }

    async fn add_member(
        repository: &crate::repository::Repository,
        org_id: uuid::Uuid,
        owner_clerk_user_id: &str,
        member_clerk_user_id: &str,
        role: OrganizationRole,
    ) -> uuid::Uuid {
        let record = repository
            .add_organization_member(
                org_id,
                owner_clerk_user_id,
                AddOrganizationMemberRequest {
                    clerk_user_id: member_clerk_user_id.to_string(),
                    name: "Member".to_string(),
                    role,
                },
            )
            .await
            .expect("member");
        record.membership.user_id
    }

    #[tokio::test]
    #[serial]
    async fn admin_can_list_members() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, _) = seed_org(&repository, "org_lm_admin", "user_owner_lm").await;
        add_member(
            &repository,
            org_id,
            "user_owner_lm",
            "user_admin_lm",
            OrganizationRole::Admin,
        )
        .await;

        let request = Request::builder()
            .method("GET")
            .uri(format!("/v1/organizations/{org_id}/members"))
            .header("X-Clerk-User-Id", "user_admin_lm")
            .header("X-Clerk-Org-Id", "org_lm_admin")
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[serial]
    async fn member_cannot_list_members() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, _) = seed_org(&repository, "org_lm_member", "user_owner_lm2").await;
        add_member(
            &repository,
            org_id,
            "user_owner_lm2",
            "user_member_lm",
            OrganizationRole::Member,
        )
        .await;

        let request = Request::builder()
            .method("GET")
            .uri(format!("/v1/organizations/{org_id}/members"))
            .header("X-Clerk-User-Id", "user_member_lm")
            .header("X-Clerk-Org-Id", "org_lm_member")
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn admin_can_update_member_role() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, _) = seed_org(&repository, "org_ur_admin", "user_owner_ur").await;
        let member_user_id = add_member(
            &repository,
            org_id,
            "user_owner_ur",
            "user_member_ur",
            OrganizationRole::Member,
        )
        .await;
        // Also add a second admin so last-admin guard won't trigger
        add_member(
            &repository,
            org_id,
            "user_owner_ur",
            "user_admin_ur",
            OrganizationRole::Admin,
        )
        .await;

        let body = serde_json::to_vec(&serde_json::json!({"role": "admin"})).expect("json");
        let request = Request::builder()
            .method("PATCH")
            .uri(format!(
                "/v1/organizations/{org_id}/members/{member_user_id}"
            ))
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_admin_ur")
            .header("X-Clerk-Org-Id", "org_ur_admin")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[serial]
    async fn member_cannot_update_member_role() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, _) = seed_org(&repository, "org_ur_member", "user_owner_ur2").await;
        let other_user_id = add_member(
            &repository,
            org_id,
            "user_owner_ur2",
            "user_other_ur",
            OrganizationRole::Member,
        )
        .await;
        add_member(
            &repository,
            org_id,
            "user_owner_ur2",
            "user_member_ur2",
            OrganizationRole::Member,
        )
        .await;

        let body = serde_json::to_vec(&serde_json::json!({"role": "admin"})).expect("json");
        let request = Request::builder()
            .method("PATCH")
            .uri(format!(
                "/v1/organizations/{org_id}/members/{other_user_id}"
            ))
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_member_ur2")
            .header("X-Clerk-Org-Id", "org_ur_member")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn last_admin_guard_blocks_downgrade_of_only_admin() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, _) = seed_org(&repository, "org_lag", "user_owner_lag").await;
        let admin_user_id = add_member(
            &repository,
            org_id,
            "user_owner_lag",
            "user_admin_lag",
            OrganizationRole::Admin,
        )
        .await;

        // org_lag has 1 owner + 1 admin; downgrading the only admin → blocked
        let body = serde_json::to_vec(&serde_json::json!({"role": "member"})).expect("json");
        let request = Request::builder()
            .method("PATCH")
            .uri(format!(
                "/v1/organizations/{org_id}/members/{admin_user_id}"
            ))
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_owner_lag")
            .header("X-Clerk-Org-Id", "org_lag")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[serial]
    async fn last_admin_guard_allows_downgrade_when_another_admin_exists() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, _) = seed_org(&repository, "org_lag2", "user_owner_lag2").await;
        let admin_user_id = add_member(
            &repository,
            org_id,
            "user_owner_lag2",
            "user_admin_lag2a",
            OrganizationRole::Admin,
        )
        .await;
        // Add a second admin so the guard allows the downgrade
        add_member(
            &repository,
            org_id,
            "user_owner_lag2",
            "user_admin_lag2b",
            OrganizationRole::Admin,
        )
        .await;

        let body = serde_json::to_vec(&serde_json::json!({"role": "member"})).expect("json");
        let request = Request::builder()
            .method("PATCH")
            .uri(format!(
                "/v1/organizations/{org_id}/members/{admin_user_id}"
            ))
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_owner_lag2")
            .header("X-Clerk-Org-Id", "org_lag2")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[test]
fn decodes_go_log_stack_and_headers_into_event() {
    let payload = GoLogPayload {
        log: "panic happened".to_string(),
        level: "error".to_string(),
        file: Some("main.go".to_string()),
        line: Some("42".to_string()),
        line_number: Some(42),
        log_fmt: None,
        stack: Some("Z29yb3V0aW5lIDEgW3J1bm5pbmdd".to_string()),
    };

    let event = map_go_log_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.level, Severity::Error);
    assert_eq!(event.message, "panic happened");
    assert!(event.stacktrace.expect("stacktrace").contains("main.go:42"));
}

#[test]
fn maps_go_bug_payload_into_stacktrace() {
    let payload = GoBugPayload {
        bug: json!("panic: test"),
        raw: json!("frame 1\nframe 2"),
        bug_line: Some("main.go:42".to_string()),
        file: Some("main.go".to_string()),
        line: Some("42".to_string()),
        line_number: Some(42),
        level: "panic".to_string(),
    };

    let event = map_go_bug_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.level, Severity::Fatal);
    assert!(event.stacktrace.contains("frame 1"));
    assert!(event.stacktrace.contains("panic: test"));
}

#[test]
fn leaves_plain_strings_untouched() {
    assert_eq!(
        decode_go_bytes("plain text"),
        Some("plain text".to_string())
    );
    assert_eq!(
        format_location(Some("main.go"), Some("99")),
        Some("main.go:99".to_string())
    );
}

#[test]
fn accepts_rust_log_payload_shape() {
    let payload = GoLogPayload {
        log: "db timeout".to_string(),
        level: "error".to_string(),
        file: Some("src/main.rs".to_string()),
        line: Some("27".to_string()),
        line_number: Some(27),
        log_fmt: Some("path=src/main.rs level=error msg=\"db timeout\" line=27".to_string()),
        stack: Some(
            "0: app::worker::run\nat /workspace/src/worker.rs:42:7\n1: std::rt::lang_start"
                .to_string(),
        ),
    };

    let event = map_go_log_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.level, Severity::Error);
    let stacktrace = event.stacktrace.expect("stacktrace");
    assert!(stacktrace.contains("src/main.rs:27"));
    assert!(stacktrace.contains("app::worker::run"));
    assert!(stacktrace.contains("/workspace/src/worker.rs:42:7"));
}

#[test]
fn accepts_rust_bug_payload_shape() {
    let payload = GoBugPayload {
        bug: json!("panic: worker crashed\n\n -> app::worker::run"),
        raw: json!("0: app::worker::run\nat /workspace/src/worker.rs:42:7"),
        bug_line: Some("/workspace/src/worker.rs:42:7".to_string()),
        file: Some("/workspace/src/worker.rs".to_string()),
        line: Some("42".to_string()),
        line_number: Some(42),
        level: "crash".to_string(),
    };

    let event = map_go_bug_payload(
        AgentAuth {
            key: "key".to_string(),
            secret: "secret".to_string(),
        },
        payload,
    )
    .expect("event");

    assert_eq!(event.level, Severity::Fatal);
    assert!(event.stacktrace.contains("worker crashed"));
    assert!(event.stacktrace.contains("/workspace/src/worker.rs:42:7"));
}

#[test]
fn preserves_canonical_stacktrace_payload_fields() {
    let payload = StacktraceEventPayload {
        agent_key: "key".to_string(),
        agent_secret: Some("secret".to_string()),
        language: "go".to_string(),
        stacktrace: "panic: test".to_string(),
        level: Severity::Error,
        occurred_at: None,
        service: Some("api".to_string()),
        environment: Some("prod".to_string()),
        attributes: std::collections::HashMap::from([(
            "source".to_string(),
            "stacktrace_api".to_string(),
        )]),
    };

    let event = payload.into_stacktrace_event();

    assert_eq!(event.agent_key, "key");
    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.service.as_deref(), Some("api"));
    assert_eq!(event.environment.as_deref(), Some("prod"));
    assert_eq!(
        event.attributes.get("source").map(String::as_str),
        Some("stacktrace_api")
    );
}

#[test]
fn preserves_authenticated_stacktrace_payload_fields() {
    let payload = AuthenticatedStacktraceEventPayload {
        language: "rust".to_string(),
        stacktrace: "panic: test".to_string(),
        level: Severity::Error,
        occurred_at: None,
        service: Some("api".to_string()),
        environment: Some("prod".to_string()),
        attributes: std::collections::HashMap::from([(
            "source".to_string(),
            "header_stacktrace_api".to_string(),
        )]),
    };

    let event = payload.into_stacktrace_event("key".to_string(), "secret".to_string());

    assert_eq!(event.agent_key, "key");
    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.service.as_deref(), Some("api"));
    assert_eq!(event.environment.as_deref(), Some("prod"));
}

#[test]
fn preserves_canonical_log_payload_fields() {
    let payload = LogEventPayload {
        language: "rust".to_string(),
        message: "db timeout".to_string(),
        stacktrace: Some("frame_one".to_string()),
        level: Severity::Warn,
        occurred_at: None,
        service: Some("api".to_string()),
        environment: Some("prod".to_string()),
        attributes: std::collections::HashMap::from([(
            "source".to_string(),
            "log_api".to_string(),
        )]),
    };

    let event = payload.into_log_event("key".to_string(), "secret".to_string());

    assert_eq!(event.agent_key, "key");
    assert_eq!(event.agent_secret.as_deref(), Some("secret"));
    assert_eq!(event.message, "db timeout");
    assert_eq!(event.stacktrace.as_deref(), Some("frame_one"));
}

#[test]
fn extracts_current_clerk_user_id_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "X-Clerk-User-Id",
        HeaderValue::from_static("user_3Ax62HMHNfbC2gyvCzBOMfB8tdb"),
    );

    let user_id = extract_current_clerk_user_id(&headers).expect("clerk user id");

    assert_eq!(user_id, "user_3Ax62HMHNfbC2gyvCzBOMfB8tdb");
}

#[test]
fn requires_current_clerk_user_id_header() {
    let error = extract_current_clerk_user_id(&HeaderMap::new()).expect_err("missing header");

    assert_eq!(
        error.to_string(),
        "validation failed: missing X-Clerk-User-Id header"
    );
}

#[test]
fn extracts_required_clerk_org_id_header() {
    let mut headers = HeaderMap::new();
    headers.insert("X-Clerk-Org-Id", HeaderValue::from_static("org_123"));

    let org_id = extract_required_clerk_org_id(&headers).expect("clerk org id");

    assert_eq!(org_id, "org_123");
}

#[test]
fn requires_clerk_org_id_header() {
    let error = extract_required_clerk_org_id(&HeaderMap::new()).expect_err("missing header");

    assert_eq!(
        error.to_string(),
        "validation failed: missing X-Clerk-Org-Id header"
    );
}

#[test]
fn finds_first_meaningful_line_after_blanks() {
    assert_eq!(
        first_meaningful_line("\n  \n panic: worker crashed\nnext line"),
        Some("panic: worker crashed".to_string())
    );
}

#[test]
fn first_meaningful_line_returns_none_for_blank_input() {
    assert_eq!(first_meaningful_line(" \n\t\n"), None);
}

#[test]
fn maps_severity_to_dashboard_tone() {
    assert_eq!(severity_to_tone("fatal"), "critical");
    assert_eq!(severity_to_tone("error"), "critical");
    assert_eq!(severity_to_tone("warn"), "warn");
    assert_eq!(severity_to_tone("info"), "good");
    assert_eq!(severity_to_tone("debug"), "good");
    assert_eq!(severity_to_tone("trace"), "neutral");
}

#[test]
fn allows_dashboard_access_for_matching_org_membership() {
    let now = Utc::now();
    let access = OrganizationAccess {
        organization: Organization {
            id: Uuid::new_v4(),
            name: "Acme".to_string(),
            clerk_org_id: Some("org_acme".to_string()),
            created_at: now,
            updated_at: now,
        },
        membership: Membership {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role: OrganizationRole::Owner,
            created_at: now,
            updated_at: now,
        },
    };

    ensure_user_can_access_clerk_org(&[access], "org_acme").expect("authorized");
}

#[test]
fn rejects_dashboard_access_for_non_member_org() {
    let now = Utc::now();
    let access = OrganizationAccess {
        organization: Organization {
            id: Uuid::new_v4(),
            name: "Acme".to_string(),
            clerk_org_id: Some("org_acme".to_string()),
            created_at: now,
            updated_at: now,
        },
        membership: Membership {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role: OrganizationRole::Owner,
            created_at: now,
            updated_at: now,
        },
    };

    let error =
        ensure_user_can_access_clerk_org(&[access], "org_other").expect_err("missing membership");

    assert_eq!(
        error.to_string(),
        "forbidden: authenticated user is not a member of organization org_other"
    );
}

fn make_org_access(role: OrganizationRole) -> OrganizationAccess {
    let now = Utc::now();
    OrganizationAccess {
        organization: Organization {
            id: Uuid::new_v4(),
            name: "Acme".to_string(),
            clerk_org_id: Some("org_acme".to_string()),
            created_at: now,
            updated_at: now,
        },
        membership: Membership {
            id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role,
            created_at: now,
            updated_at: now,
        },
    }
}

#[test]
fn owner_passes_all_permission_checks() {
    let access = make_org_access(OrganizationRole::Owner);
    for perm in [
        Permission::ReadBugs,
        Permission::WriteBugs,
        Permission::ManageAgents,
        Permission::ManageProviders,
        Permission::ManageMembers,
        Permission::ManageOrganization,
        Permission::ManageApiKeys,
    ] {
        check_org_permission(std::slice::from_ref(&access), "org_acme", perm)
            .unwrap_or_else(|_| panic!("Owner should pass {perm:?}"));
    }
}

#[test]
fn admin_passes_operational_permissions() {
    let access = make_org_access(OrganizationRole::Admin);
    for perm in [
        Permission::ReadBugs,
        Permission::WriteBugs,
        Permission::ManageAgents,
        Permission::ManageProviders,
        Permission::ManageMembers,
        Permission::ManageApiKeys,
    ] {
        check_org_permission(std::slice::from_ref(&access), "org_acme", perm)
            .unwrap_or_else(|_| panic!("Admin should pass {perm:?}"));
    }
}

#[test]
fn admin_blocked_from_manage_organization() {
    let access = make_org_access(OrganizationRole::Admin);
    let err = check_org_permission(&[access], "org_acme", Permission::ManageOrganization)
        .expect_err("Admin should be blocked from ManageOrganization");
    assert!(err.to_string().contains("forbidden"));
}

#[test]
fn member_passes_read_bugs() {
    let access = make_org_access(OrganizationRole::Member);
    check_org_permission(&[access], "org_acme", Permission::ReadBugs)
        .expect("Member should pass ReadBugs");
}

#[test]
fn member_blocked_from_write_and_management_permissions() {
    let access = make_org_access(OrganizationRole::Member);
    for perm in [
        Permission::WriteBugs,
        Permission::ManageAgents,
        Permission::ManageProviders,
        Permission::ManageMembers,
        Permission::ManageOrganization,
        Permission::ManageApiKeys,
    ] {
        check_org_permission(std::slice::from_ref(&access), "org_acme", perm)
            .expect_err(&format!("Member should be blocked from {perm:?}"));
    }
}

#[test]
fn non_member_gets_forbidden_not_validation_error() {
    let access = make_org_access(OrganizationRole::Member);
    let err = check_org_permission(&[access], "org_other", Permission::ReadBugs)
        .expect_err("non-member should be forbidden");
    assert!(
        err.to_string().contains("forbidden"),
        "expected forbidden, got: {err}"
    );
}

mod api_key_management {
    use axum::body::Body;
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use serial_test::serial;
    use tower::ServiceExt;

    use crate::domain::{
        AddOrganizationMemberRequest, CreateAccountRequest, CreateOrganizationRequest,
        NotificationProvider, OrganizationRole, Severity, TicketProvider,
    };
    use crate::test_support::{build_test_app, reset_database};

    async fn seed_org_and_account(
        repository: &crate::repository::Repository,
        clerk_org_id: &str,
        owner_clerk_user_id: &str,
    ) -> (uuid::Uuid, uuid::Uuid) {
        let org = repository
            .create_organization(CreateOrganizationRequest {
                name: "API Key Org".to_string(),
                clerk_org_id: Some(clerk_org_id.to_string()),
                owner_clerk_user_id: owner_clerk_user_id.to_string(),
                owner_name: "Owner".to_string(),
            })
            .await
            .expect("org");

        let account = repository
            .create_account(CreateAccountRequest {
                organization_id: Some(org.organization.id),
                name: "API Key Account".to_string(),
                create_tickets: false,
                ticket_provider: TicketProvider::None,
                ticketing_api_key: None,
                notification_provider: NotificationProvider::None,
                notification_api_key: None,
                ai_enabled: false,
                use_managed_ai: false,
                ai_api_key: None,
                notify_min_level: Severity::Error,
                rapid_occurrence_window_minutes: 30,
                rapid_occurrence_threshold: 3,
            })
            .await
            .expect("account");

        (org.organization.id, account.id)
    }

    #[tokio::test]
    #[serial]
    async fn member_can_create_dev_key() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, account_id) =
            seed_org_and_account(&repository, "org_ak_dev", "user_ak_owner").await;
        repository
            .add_organization_member(
                org_id,
                "user_ak_owner",
                AddOrganizationMemberRequest {
                    clerk_user_id: "user_ak_member".to_string(),
                    name: "Member".to_string(),
                    role: OrganizationRole::Member,
                },
            )
            .await
            .expect("member");

        let body = serde_json::to_vec(&serde_json::json!({
            "name": "My Dev Key",
            "key_type": "dev",
            "account_id": account_id
        }))
        .expect("json");

        let request = Request::builder()
            .method("POST")
            .uri("/v1/api-keys")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_ak_member")
            .header("X-Clerk-Org-Id", "org_ak_dev")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert!(json.get("api_secret").is_some());
        assert_eq!(json["key_type"], "dev");
        assert_eq!(json["scope"], "ingest");
    }

    #[tokio::test]
    #[serial]
    async fn member_cannot_create_system_key() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, account_id) =
            seed_org_and_account(&repository, "org_ak_sys", "user_ak_sys_owner").await;
        repository
            .add_organization_member(
                org_id,
                "user_ak_sys_owner",
                AddOrganizationMemberRequest {
                    clerk_user_id: "user_ak_sys_member".to_string(),
                    name: "Member".to_string(),
                    role: OrganizationRole::Member,
                },
            )
            .await
            .expect("member");

        let body = serde_json::to_vec(&serde_json::json!({
            "name": "System Key",
            "key_type": "system",
            "scope": "ingest",
            "account_id": account_id
        }))
        .expect("json");

        let request = Request::builder()
            .method("POST")
            .uri("/v1/api-keys")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_ak_sys_member")
            .header("X-Clerk-Org-Id", "org_ak_sys")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[serial]
    async fn admin_can_create_system_key() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, account_id) =
            seed_org_and_account(&repository, "org_ak_admin_sys", "user_ak_admin_sys_owner").await;
        repository
            .add_organization_member(
                org_id,
                "user_ak_admin_sys_owner",
                AddOrganizationMemberRequest {
                    clerk_user_id: "user_ak_admin_sys".to_string(),
                    name: "Admin".to_string(),
                    role: OrganizationRole::Admin,
                },
            )
            .await
            .expect("admin");

        let body = serde_json::to_vec(&serde_json::json!({
            "name": "Prod Ingest",
            "key_type": "system",
            "scope": "ingest",
            "account_id": account_id,
            "environment": "production"
        }))
        .expect("json");

        let request = Request::builder()
            .method("POST")
            .uri("/v1/api-keys")
            .header("Content-Type", "application/json")
            .header("X-Clerk-User-Id", "user_ak_admin_sys")
            .header("X-Clerk-Org-Id", "org_ak_admin_sys")
            .body(Body::from(body))
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["key_type"], "system");
        assert_eq!(json["scope"], "ingest");
        assert_eq!(json["environment"], "production");
    }

    #[tokio::test]
    #[serial]
    async fn admin_can_revoke_api_key() {
        let (app, repository) = build_test_app().await;
        reset_database().await;
        let (org_id, account_id) =
            seed_org_and_account(&repository, "org_ak_revoke", "user_ak_revoke_owner").await;

        let key = repository
            .create_api_key(
                org_id,
                None,
                crate::domain::CreateApiKeyRequest {
                    name: "Revocable".to_string(),
                    key_type: crate::domain::ApiKeyType::System,
                    scope: Some(crate::domain::ApiKeyScope::Ingest),
                    account_id: Some(account_id),
                    environment: None,
                    expires_at: None,
                },
            )
            .await
            .expect("key");

        let request = Request::builder()
            .method("DELETE")
            .uri(format!("/v1/api-keys/{}", key.api_key.id))
            .header("X-Clerk-User-Id", "user_ak_revoke_owner")
            .header("X-Clerk-Org-Id", "org_ak_revoke")
            .body(Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }
}
