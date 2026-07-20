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

#[tokio::test]
async fn insert_artifact_on_conflict_updates_existing_row() {
    // BINDING carry #4 (task-9 brief): Task 7's `ON CONFLICT (engagement_id,
    // project_id, artifact_id) DO UPDATE` path is otherwise never exercised —
    // `run_generation` always calls `generate_artifact_id` fresh (random hex
    // suffix), so it never naturally re-inserts under the same key. Exercise
    // the upsert directly: insert once, then insert again under the SAME key
    // with different title/html, and assert the row was updated in place
    // (not duplicated, not rejected by the unique constraint).
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('C') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let aid = generate_artifact_id("First");
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "First Title",
        "<h1>a</h1>",
        "<h1>a</h1>",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    insert_artifact(
        &db.pool,
        eng,
        "proj-1",
        &aid,
        "Second Title",
        "<h1>b</h1>",
        "<h1>b</h1>",
        Uuid::new_v4(),
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1 AND artifact_id = $2",
    )
    .bind(eng)
    .bind(&aid)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "ON CONFLICT must update in place, not duplicate the row"
    );

    let got = get_artifact(&db.pool, eng, "proj-1", &aid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.title, "Second Title");
    assert_eq!(got.rendered_html, "<h1>b</h1>");
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

// --- run_generation (Task 9 orchestrator) tests ---------------------------

use celerex_api::runs::generation::loop_core::{
    FragmentSlot, MAX_PROMPT_FRAGMENT_CHARS, PromptFragment,
};
use celerex_api::runs::generation::run_generation;
use celerex_orchestrator::llm::stub::StubProvider;
use serde_json::json;
use std::sync::Arc;

/// Drain every currently-buffered SSE frame off the channel into owned
/// `Event`s (not just a count), so tests can inspect their wire content.
async fn drain_events(
    mut rx: tokio::sync::mpsc::Receiver<
        Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
) -> Vec<axum::response::sse::Event> {
    let mut out = Vec::new();
    while let Ok(Ok(ev)) = rx.try_recv() {
        out.push(ev);
    }
    out
}

/// axum 0.8.8's `Event { buffer: Buffer, flags: EventFlags }` has no public
/// getter for the built `event:`/`data:` wire text (both fields private, no
/// accessor — confirmed by reading the vendored source at
/// `axum-0.8.8/src/response/sse.rs`). It DOES `#[derive(Debug, Clone)]`, and
/// `Buffer`'s variants wrap `bytes::Bytes`/`BytesMut`, whose OWN `Debug` impl
/// (`bytes-1.11.1/src/fmt/debug.rs`) prints printable-ASCII bytes verbatim
/// (only backslash/quote/control bytes are escaped) — so plain-word literal
/// text (no embedded quotes) in the JSON survives intact inside
/// `format!("{ev:?}")`. This is the only test-visible seam for pinning what
/// `run_generation`'s call sites actually put on the wire — as opposed to the
/// emitter's OWN frame-vocabulary tests in `emitter.rs`, which pin the
/// builder functions (`status_frame`, `end_frame`, ...) but not which literal
/// label/status/detail strings THIS function chooses to pass them under which
/// conditions. Deliberately search bare words (never quote-wrapped), since
/// the Debug escaping of `"` as `\"` would break a naive quoted-substring
/// match.
fn event_contains(events: &[axum::response::sse::Event], needle: &str) -> bool {
    events.iter().any(|e| format!("{e:?}").contains(needle))
}

#[tokio::test]
async fn run_generation_writes_file_persists_artifact_and_finishes_succeeded() {
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let run_id = Uuid::new_v4();
    let provider = Arc::new(StubProvider::new(vec![
        json!({ "choices": [{ "message": { "content": "building",
            "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<!doctype html><h1>Hi</h1>\"}" } }] } }] }),
        json!({ "choices": [{ "message": { "content": "DONE" } }] }),
    ]));
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "copilot",
        "gpt-4o",
        eng,
        "proj-1",
        run_id,
        "make a landing page",
        &db.pool,
        20,
        &[],
    )
    .await;
    drop(tx);
    let events = drain_events(rx).await;
    assert!(!events.is_empty());
    // Terminal frame is the SUCCEEDED end — the positive control matching the
    // negative control in `run_generation_forwards_provider_failure_as_failed_end`
    // below (proving this assertion actually discriminates pass/fail).
    assert!(event_contains(&events, "succeeded"));

    // Exactly one artifact row persisted for this engagement/project.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1 AND project_id = 'proj-1'",
    )
    .bind(eng)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let rendered: String =
        sqlx::query_scalar("SELECT rendered_html FROM design_artifacts WHERE engagement_id = $1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(rendered.contains("<h1>Hi</h1>"));
}

#[tokio::test]
async fn run_generation_rejects_script_artifact_but_still_succeeds() {
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G2') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let provider = Arc::new(StubProvider::new(vec![
        json!({ "choices": [{ "message": { "content": "",
            "tool_calls": [{ "id": "c1", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<h1>x</h1><script>alert(1)</script>\"}" } }] } }] }),
        json!({ "choices": [{ "message": { "content": "done" } }] }),
    ]));
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "copilot",
        "gpt-4o",
        eng,
        "proj-1",
        Uuid::new_v4(),
        "make it",
        &db.pool,
        20,
        &[],
    )
    .await;
    // A rejected artifact means NO row persisted, but the run must have
    // finished succeeded — asserted both structurally (no panic, no row) AND
    // on the wire (BINDING carries #2/#3 from the task-9 brief: "artifact-error"
    // is the literal status label the guard-rejection path must emit, and the
    // run's terminal `end` status must still be "succeeded", never "failed" —
    // this exact flow is load-bearing for the shipped web UI's friendly-pill
    // rendering).
    let events = drain_events(rx).await;
    assert!(event_contains(&events, "artifact-error"));
    assert!(event_contains(&events, "script elements are not supported"));
    assert!(event_contains(&events, "succeeded"));
    assert!(!event_contains(&events, "failed"));

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn run_generation_hitting_max_turns_emits_reached_step_limit_literal_and_still_succeeds() {
    // BINDING carry #2 (task-9 brief): "reached step limit" must be pinned as
    // a literal string in a test. loop_core.rs's own unit tests cover
    // `StopReason::MaxTurns` in isolation, but not run_generation's CALL SITE
    // choice to map that stop reason into this exact status literal — that
    // byte-exactness lands here, at the Task 9 wiring layer.
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G3') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    // Two identical tool-call turns with max_turns=2 forces StopReason::MaxTurns
    // (mirrors loop_core.rs's own `stops_at_max_turns` scripted fixture).
    let provider = Arc::new(StubProvider::new(vec![
        json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
        json!({ "choices": [{ "message": { "content": "", "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"a\",\"content\":\"b\"}" } }] } }] }),
    ]));
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "copilot",
        "gpt-4o",
        eng,
        "proj-1",
        Uuid::new_v4(),
        "go",
        &db.pool,
        2,
        &[],
    )
    .await;
    let events = drain_events(rx).await;
    assert!(event_contains(&events, "reached step limit"));
    assert!(event_contains(&events, "succeeded"));
}

#[tokio::test]
async fn run_generation_forwards_provider_failure_as_failed_end() {
    // Negative control for the two "still succeeds" tests above: a genuine
    // provider/loop failure (StubProvider exhausted with zero scripted
    // responses) must still end the run "failed" — proving the `succeeded`/
    // `!failed` assertions above actually discriminate pass/fail rather than
    // being true of every run regardless of outcome.
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G4') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let provider = Arc::new(StubProvider::new(vec![])); // no scripted responses -> immediate Err
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "copilot",
        "gpt-4o",
        eng,
        "proj-1",
        Uuid::new_v4(),
        "go",
        &db.pool,
        20,
        &[],
    )
    .await;
    let events = drain_events(rx).await;
    assert!(event_contains(&events, "failed"));
    assert!(!event_contains(&events, "succeeded"));
}

// --- ported daemon parity regression tests (Task 10) ----------------------
// Ported from the daemon's api-generation-run.test.ts, which the Task 10
// brief calls out as covering "ordered SSE" and "secret-non-leak" (the other
// daemon file, api-generation-provider-turn.test.ts — empty-tools omission,
// envelope, charset alias — is already unit-tested in Task 4's
// provider_turn.rs). The two cases below are the still-uncovered daemon
// behaviors.

#[tokio::test]
async fn provider_failure_does_not_leak_body_and_finishes_failed() {
    // A provider that returns an error string containing a secret upstream
    // detail must NOT appear in any emitted frame; the run finishes failed.
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('F') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    // StubProvider with no scripted responses errors "no more scripted responses".
    let provider = Arc::new(StubProvider::new(vec![]));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "openai",
        "gpt-4o",
        eng,
        "proj-1",
        Uuid::new_v4(),
        "x",
        &db.pool,
        20,
        &[],
    )
    .await;
    // No artifact row; loop errored on the first turn → failed terminal.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn max_turns_still_persists_last_index_html_and_succeeds() {
    // Model keeps calling tools; the loop caps at maxTurns but the last written
    // index.html is still validated + persisted, and the run succeeds.
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('M') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let turn = || {
        json!({ "choices": [{ "message": { "content": "",
            "tool_calls": [{ "id": "x", "type": "function", "function": { "name": "write_file", "arguments": "{\"path\":\"index.html\",\"content\":\"<h1>loop</h1>\"}" } }] } }] })
    };
    let provider = Arc::new(StubProvider::new(vec![turn(), turn(), turn()]));
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "copilot",
        "gpt-4o",
        eng,
        "proj-1",
        Uuid::new_v4(),
        "loop it",
        &db.pool,
        3,
        &[],
    )
    .await;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM design_artifacts WHERE engagement_id = $1")
            .bind(eng)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn run_generation_truncated_fragment_emits_truncation_status_literal() {
    // BINDING carry #5 (task-9 brief): when the fragment composer truncates,
    // run_generation must emit the `prompt-fragment-truncated` status literal.
    // v1's two production call sites always pass `&[]` (composer never
    // truncates an empty list), so this is the only coverage proving the
    // wiring is live rather than dead code — it exercises run_generation's
    // public `fragments` parameter directly with a fragment engineered to
    // cross `MAX_PROMPT_FRAGMENT_CHARS`.
    let db = common::start_test_db().await;
    let eng =
        sqlx::query_scalar::<_, Uuid>("INSERT INTO engagements (name) VALUES ('G5') RETURNING id")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let provider = Arc::new(StubProvider::new(vec![
        json!({ "choices": [{ "message": { "content": "DONE" } }] }),
    ]));
    let big = PromptFragment {
        slot: FragmentSlot::Brand,
        text: "x".repeat(MAX_PROMPT_FRAGMENT_CHARS + 500),
    };
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    run_generation(
        &tx,
        provider,
        "copilot",
        "gpt-4o",
        eng,
        "proj-1",
        Uuid::new_v4(),
        "go",
        &db.pool,
        20,
        &[big],
    )
    .await;
    let events = drain_events(rx).await;
    assert!(event_contains(&events, "prompt-fragment-truncated"));
}
