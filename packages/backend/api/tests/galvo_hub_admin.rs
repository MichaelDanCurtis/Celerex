mod common;
use uuid::Uuid;

/// The allowlist (`CELEREX_HUB_ADMIN_ENGAGEMENTS`) is parsed once at boot inside
/// `build_app`, so each phase sets/removes the env BEFORE constructing the app.
/// Both cases live in one sequential test to avoid cross-test env races.
#[tokio::test]
async fn hub_routes_gate_on_admin_allowlist() {
    let db = common::start_test_db().await;

    // Seed an engagement to scope the session token against.
    let eng: Uuid =
        sqlx::query_scalar("INSERT INTO engagements (name) VALUES ('Hub Admin') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let token = celerex_api::auth::jwt::mint_for_tests(eng);

    // --- Phase 1: empty/unset allowlist → deny all (403). ---
    unsafe {
        std::env::remove_var("CELEREX_HUB_ADMIN_ENGAGEMENTS");
    }
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let res = server
        .get("/api/galvo/hub/status")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    assert_eq!(
        res.status_code(),
        403,
        "unset allowlist must deny non-admin engagements"
    );

    // --- Phase 2: engagement in the allowlist → passes the gate (NOT 403). ---
    // Downstream (status) may report not-registered without a live keystore;
    // we only assert the admin gate let it through.
    unsafe {
        std::env::set_var("CELEREX_HUB_ADMIN_ENGAGEMENTS", eng.to_string());
    }
    let app = celerex_api::build_app(db.pool.clone());
    let server = axum_test::TestServer::new(app).unwrap();
    let res = server
        .get("/api/galvo/hub/status")
        .add_header("authorization", format!("Bearer {token}"))
        .await;
    assert_ne!(
        res.status_code(),
        403,
        "allowlisted engagement must pass the admin gate"
    );

    unsafe {
        std::env::remove_var("CELEREX_HUB_ADMIN_ENGAGEMENTS");
    }
}
