use std::sync::Arc;

use axum::body::Body;
use http::Request;
use http_body_util::BodyExt;
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};
use tower::ServiceExt;

use tephra_server::api::AppState;
use tephra_server::monitor::{self, SystemState};

fn test_app_state() -> Arc<AppState> {
    let n_cores = monitor::get_cpu_count();
    let max_freq = monitor::get_max_freq();
    let (sse_tx, _) = broadcast::channel(64);
    Arc::new(AppState {
        system_state: RwLock::new(SystemState::new(n_cores, max_freq)),
        sse_tx,
        interval_ms: 500,
    })
}

async fn get_json(state: &Arc<AppState>, path: &str) -> Value {
    let app = tephra_server::api::router(Arc::clone(state));
    let response = app
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

// ── Health ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let state = test_app_state();
    let json = get_json(&state, "/health").await;
    assert_eq!(json["status"], "ok");
}

// ── System Info ──────────────────────────────────────────────────────────

#[tokio::test]
async fn system_info_has_required_fields() {
    let state = test_app_state();
    let json = get_json(&state, "/api/v1/system").await;

    assert!(json["hostname"].is_string());
    assert!(json["cpu_model"].is_string());
    assert!(json["core_count"].is_number());
    assert!(json["max_freq_mhz"].is_number());
    assert!(json["scaling_driver"].is_string());
    assert!(json["governor"].is_string());
    assert!(json["ram_gb"].is_number());
    assert!(json["agent_version"].is_string());

    // Core count should be positive
    assert!(json["core_count"].as_u64().unwrap() > 0);
    // RAM should be positive
    assert!(json["ram_gb"].as_f64().unwrap() > 0.0);
}

// ── Snapshot ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn snapshot_has_required_fields() {
    let state = test_app_state();

    // Do one sample so we have data
    {
        let mut s = state.system_state.write().await;
        s.sample();
    }

    let json = get_json(&state, "/api/v1/snapshot").await;

    assert!(json["timestamp_ms"].is_number());
    assert!(json["temp_c"].is_number());
    assert!(json["temp_rate_cs"].is_number());
    assert!(json["ppt_watts"].is_number());
    assert!(json["avg_freq_mhz"].is_number());
    assert!(json["avg_util_pct"].is_number());
    assert!(json["fan_rpm"].is_number());
    assert!(json["fan_detected"].is_boolean());
    assert!(json["throttle_active"].is_boolean());
    assert!(json["throttle_reason"].is_string());
    assert!(json["peak_temp"].is_number());
    assert!(json["peak_ppt"].is_number());
    assert!(json["peak_freq"].is_number());
    assert!(json["peak_fan"].is_number());
    assert!(json["thermal_events"].is_number());
    assert!(json["power_events"].is_number());
    assert!(json["energy_wh"].is_number());
    assert!(json["uptime_secs"].is_number());
    assert!(json["cores"].is_array());
}

#[tokio::test]
async fn snapshot_core_count_matches_system() {
    let state = test_app_state();
    {
        let mut s = state.system_state.write().await;
        s.sample();
    }

    let system = get_json(&state, "/api/v1/system").await;
    let snapshot = get_json(&state, "/api/v1/snapshot").await;

    let expected_cores = system["core_count"].as_u64().unwrap();
    let actual_cores = snapshot["cores"].as_array().unwrap().len() as u64;
    assert_eq!(actual_cores, expected_cores);
}

#[tokio::test]
async fn snapshot_cores_have_freq_and_util() {
    let state = test_app_state();
    {
        let mut s = state.system_state.write().await;
        s.sample();
    }

    let json = get_json(&state, "/api/v1/snapshot").await;
    let cores = json["cores"].as_array().unwrap();

    for core in cores {
        assert!(core["freq_mhz"].is_number());
        assert!(core["util_pct"].is_number());
    }
}

// ── History ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn history_has_required_fields() {
    let state = test_app_state();
    let json = get_json(&state, "/api/v1/history").await;

    assert_eq!(json["interval_ms"], 500);
    assert!(json["samples"].is_number());
    assert!(json["temp_c"].is_array());
    assert!(json["avg_freq_mhz"].is_array());
    assert!(json["ppt_watts"].is_array());
    assert!(json["avg_util_pct"].is_array());
    assert!(json["fan_rpm"].is_array());
}

#[tokio::test]
async fn history_arrays_same_length() {
    let state = test_app_state();

    // Take a few samples to build history
    {
        let mut s = state.system_state.write().await;
        for _ in 0..5 {
            s.sample();
        }
    }

    let json = get_json(&state, "/api/v1/history").await;
    let samples = json["samples"].as_u64().unwrap() as usize;

    assert_eq!(json["temp_c"].as_array().unwrap().len(), samples);
    assert_eq!(json["avg_freq_mhz"].as_array().unwrap().len(), samples);
    assert_eq!(json["ppt_watts"].as_array().unwrap().len(), samples);
    assert_eq!(json["avg_util_pct"].as_array().unwrap().len(), samples);
    assert_eq!(json["fan_rpm"].as_array().unwrap().len(), samples);
}

// ── OpenAPI Spec ─────────────────────────────────────────────────────────

#[tokio::test]
async fn openapi_spec_is_valid() {
    let state = test_app_state();
    let json = get_json(&state, "/api/v1/openapi.json").await;

    assert_eq!(json["openapi"], "3.1.0");
    assert_eq!(json["info"]["title"], "Tephra API");
    assert!(json["paths"].is_object());
    assert!(json["components"]["schemas"].is_object());
}

#[tokio::test]
async fn openapi_spec_documents_all_endpoints() {
    let state = test_app_state();
    let json = get_json(&state, "/api/v1/openapi.json").await;

    let paths = json["paths"].as_object().unwrap();
    assert!(paths.contains_key("/health"));
    assert!(paths.contains_key("/api/v1/system"));
    assert!(paths.contains_key("/api/v1/snapshot"));
    assert!(paths.contains_key("/api/v1/history"));
    assert!(paths.contains_key("/api/v1/events"));
    assert!(paths.contains_key("/api/v1/openapi.json"));
}

// ── SSE Events ───────────────────────────────────────────────────────────

#[tokio::test]
async fn sse_events_endpoint_returns_stream() {
    let state = test_app_state();
    let app = tephra_server::api::router(Arc::clone(&state));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert!(response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/event-stream"));
}

// ── 404 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let state = test_app_state();
    let app = tephra_server::api::router(Arc::clone(&state));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

// ── Snapshot values are sane ─────────────────────────────────────────────

#[tokio::test]
async fn snapshot_throttle_reason_is_valid() {
    let state = test_app_state();
    {
        let mut s = state.system_state.write().await;
        s.sample();
    }

    let json = get_json(&state, "/api/v1/snapshot").await;
    let reason = json["throttle_reason"].as_str().unwrap();
    assert!(
        ["none", "thermal", "power"].contains(&reason),
        "unexpected throttle_reason: {}",
        reason
    );
}

#[tokio::test]
async fn snapshot_util_in_range() {
    let state = test_app_state();
    {
        let mut s = state.system_state.write().await;
        s.sample();
    }

    let json = get_json(&state, "/api/v1/snapshot").await;
    let util = json["avg_util_pct"].as_f64().unwrap();
    assert!(util >= 0.0 && util <= 100.0);
}
