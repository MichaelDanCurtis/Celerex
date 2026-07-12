mod common;

use celerex_api::runs::generation::artifact_store::{
    generate_artifact_id, get_artifact, insert_artifact,
};
use uuid::Uuid;

#[tokio::test]
async fn insert_then_get_roundtrips_scoped_by_engagement() {
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('A') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let other =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('B') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let run_id = Uuid::new_v4();
    let aid = generate_artifact_id("My Landing Page");
    assert!(aid.starts_with("la-"));
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "My Landing Page",
        "<h1>t</h1>",
        "<h1>t</h1>",
        run_id,
    )
    .await
    .unwrap();

    let got = get_artifact(&db.pool, eng, "proj-1", &aid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.title, "My Landing Page");
    assert_eq!(got.rendered_html, "<h1>t</h1>");

    // Cross-engagement read returns None (isolation).
    assert!(
        get_artifact(&db.pool, other, "proj-1", &aid)
            .await
            .unwrap()
            .is_none()
    );
}

// --- artifact route tests -------------------------------------------------
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::util::ServiceExt;

#[tokio::test]
async fn preview_route_serves_rendered_html_scoped_to_engagement() {
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let aid = generate_artifact_id("Page");
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "Page",
        "<h1>t</h1>",
        "<h1>rendered</h1>",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let token = common::mint_token(eng);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/live-artifacts/{aid}/preview?projectId=proj-1&variant=rendered"
                ))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(&body[..], b"<h1>rendered</h1>");
}

#[tokio::test]
async fn meta_route_404s_for_other_engagement() {
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let other = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO engagements (name) VALUES ('Other') RETURNING id",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let aid = generate_artifact_id("Page");
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "Page",
        "<h1>t</h1>",
        "<h1>t</h1>",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let token = common::mint_token(other);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/live-artifacts/{aid}?projectId=proj-1"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn preview_route_sets_csp_parity_headers_matching_daemon() {
    // Serve-layer CSP parity with the daemon's setLiveArtifactPreviewHeaders
    // (celerex-design/apps/daemon/src/server.ts:3610-3630). The stored rendered
    // HTML deliberately carries a guard-gap payload (`<svg/onload=...>`): this
    // serve does NOT re-run the Task 3 guard, so `script-src 'none'` + the
    // script-less `sandbox` are the enforcement boundary that makes it inert.
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let aid = generate_artifact_id("Page");
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "Page",
        "<h1>t</h1>",
        "<svg/onload=alert(1)></svg>",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let token = common::mint_token(eng);
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/live-artifacts/{aid}/preview?projectId=proj-1&variant=rendered"
                ))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let h = resp.headers();
    // Byte-for-byte CSP parity (server.ts:3615-3630), directive order preserved.
    assert_eq!(
        h.get("content-security-policy").unwrap().to_str().unwrap(),
        "default-src 'none'; base-uri 'none'; script-src 'none'; object-src 'none'; connect-src 'none'; form-action 'none'; frame-ancestors 'self'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'unsafe-inline'; sandbox allow-same-origin"
    );
    // The load-bearing controls the guard story depends on (spec §5.2/§5.4.1).
    let csp = h.get("content-security-policy").unwrap().to_str().unwrap();
    assert!(csp.contains("script-src 'none'"));
    assert!(csp.contains("sandbox allow-same-origin") && !csp.contains("allow-scripts"));
    assert!(csp.contains("img-src 'self' data: blob:"));
    assert!(csp.contains("connect-src 'none'"));
    assert_eq!(
        h.get("x-content-type-options").unwrap().to_str().unwrap(),
        "nosniff"
    );
    assert_eq!(
        h.get("referrer-policy").unwrap().to_str().unwrap(),
        "no-referrer"
    );
    assert_eq!(
        h.get("cache-control").unwrap().to_str().unwrap(),
        "no-store"
    );
    // Content-Type stays owned by Html (server.ts:3611), not overridden by the prefix.
    assert_eq!(
        h.get("content-type").unwrap().to_str().unwrap(),
        "text/html; charset=utf-8"
    );
}

#[tokio::test]
async fn meta_route_returns_artifact_json_with_csp_parity_headers() {
    // 200-path coverage for the META route (`get_artifact_meta`). Until now only
    // the 404 case (`meta_route_404s_for_other_engagement` above) and the
    // PREVIEW route's 200 path (`preview_route_sets_csp_parity_headers_matching_daemon`
    // above) were under test, so the claims that META (a) carries the same
    // CSP-parity headers as PREVIEW and (b) returns the documented JSON body
    // shape were true by inspection only.
    let db = common::start_test_db().await;
    let (app, eng) = common::build_app_for_generation_tests(db.pool.clone()).await;
    let aid = generate_artifact_id("Page");
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "Page",
        "<h1>t</h1>",
        "<h1>rendered</h1>",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let token = common::mint_token(eng);

    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/live-artifacts/{aid}?projectId=proj-1"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Same byte-for-byte CSP parity asserted for the preview route in
    // `preview_route_sets_csp_parity_headers_matching_daemon` above — both
    // routes share the same `live_artifact_security_headers()` source, so they
    // must match exactly.
    let h = resp.headers();
    assert_eq!(
        h.get("content-security-policy").unwrap().to_str().unwrap(),
        "default-src 'none'; base-uri 'none'; script-src 'none'; object-src 'none'; connect-src 'none'; form-action 'none'; frame-ancestors 'self'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'unsafe-inline'; sandbox allow-same-origin"
    );
    assert_eq!(
        h.get("x-content-type-options").unwrap().to_str().unwrap(),
        "nosniff"
    );
    assert_eq!(
        h.get("referrer-policy").unwrap().to_str().unwrap(),
        "no-referrer"
    );
    assert_eq!(
        h.get("cache-control").unwrap().to_str().unwrap(),
        "no-store"
    );
    // Content-Type stays owned by Json (server.ts:3611 parity note), not
    // overridden by the security-header prefix.
    assert_eq!(
        h.get("content-type").unwrap().to_str().unwrap(),
        "application/json"
    );

    // Body shape: pin the exact fields `get_artifact_meta` emits.
    let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["artifact"]["id"], aid);
    assert_eq!(json["artifact"]["projectId"], "proj-1");
    assert_eq!(json["artifact"]["title"], "Page");
    assert_eq!(json["artifact"]["refreshStatus"], "idle");
    assert_eq!(json["artifact"]["preview"]["type"], "html");
    assert_eq!(json["artifact"]["preview"]["entry"], "index.html");

    // Cheap negative: the meta body must not leak the rendered HTML itself.
    let body_str = String::from_utf8_lossy(&body);
    assert!(!body_str.contains("<h1>rendered</h1>"));
}
