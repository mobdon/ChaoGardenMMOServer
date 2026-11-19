use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chao_game_server::{
    metrics::Metrics,
    service::{AppState, Config, router},
};
use http_body_util::BodyExt;
use std::time::Duration;
use tempfile::TempDir;
use tower::ServiceExt;

fn value(text: &str, name: &str) -> u64 {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{name} ")))
        .unwrap_or_else(|| panic!("Missing metric {name}"))
        .parse()
        .unwrap()
}

#[test]
fn metrics_preserve_shared_counters_connection_gauge_and_cumulative_histogram() {
    let metrics = Metrics::new();
    let worker = metrics.clone();
    std::thread::spawn(move || {
        worker.record_request(200, Duration::from_millis(5));
        worker.record_request(503, Duration::from_millis(75));
        worker.record_request(404, Duration::from_secs(11));
        worker.ws_opened(false);
        worker.ws_opened(true);
        worker.ws_closed();
        worker.record_persist_failure();
        worker.record_event_drop();
    })
    .join()
    .unwrap();
    let text = metrics.render();
    assert_eq!(value(&text, "chao_http_requests_total"), 3);
    assert_eq!(value(&text, "chao_http_errors_total"), 1);
    assert_eq!(value(&text, "chao_ws_connects_total"), 2);
    assert_eq!(value(&text, "chao_ws_connections"), 1);
    assert_eq!(value(&text, "chao_reconnects_total"), 1);
    assert_eq!(value(&text, "chao_persist_failures_total"), 1);
    assert_eq!(value(&text, "chao_event_drops_total"), 1);
    for (bound, expected) in [
        ("0.005", 1),
        ("0.05", 1),
        ("0.1", 2),
        ("10", 2),
        ("+Inf", 3),
    ] {
        assert_eq!(
            value(
                &text,
                &format!("chao_http_request_duration_seconds_bucket{{le=\"{bound}\"}}")
            ),
            expected,
            "bucket {bound} must include every smaller bucket"
        );
    }
    assert_eq!(value(&text, "chao_http_request_duration_seconds_count"), 3);
    metrics.ws_closed();
    assert_eq!(value(&metrics.render(), "chao_ws_connections"), 0);
}

#[tokio::test]
async fn metrics_route_exposes_actual_router_requests_without_creating_profiles() {
    let directory = TempDir::new().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let app = router(state.clone());
    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/plain; version=0.0.4"
    );
    let body = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert_eq!(value(&body, "chao_http_requests_total"), 1);
    assert_eq!(value(&body, "chao_http_errors_total"), 0);
    assert_eq!(value(&body, "chao_ws_connections"), 0);
    assert!(!directory.path().join("profiles.json").exists());
    assert!(state.game.lock().unwrap().players.is_empty());
}
