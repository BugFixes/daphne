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

    use crate::domain::{AddOrganizationMemberRequest, CreateOrganizationRequest, OrganizationRole};
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
