//! Minimal metrics for operations, exported in Prometheus text format at
//! `/metrics`. Deliberately dependency-free: an MVP budget does not justify a
//! full `metrics` facade, and these are exactly the series the alerts in
//! `infrastructure/monitoring/alerts.yml` consume.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Latency bucket upper bounds in seconds for `chao_http_request_duration`.
pub const LATENCY_BUCKETS: &[f64] = &[0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

/// Counters and gauges plus a coarse latency histogram. All updates are
/// lock-free; counters may only increase, gauges may move both ways.
#[derive(Debug)]
pub struct Metrics {
    started: Instant,
    requests_total: AtomicU64,
    errors_total: AtomicU64,
    ws_connections: AtomicU64,
    ws_connects_total: AtomicU64,
    persist_failures_total: AtomicU64,
    event_drops_total: AtomicU64,
    latency_buckets: [AtomicU64; LATENCY_BUCKETS.len()],
    latency_infinite: AtomicU64,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            requests_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            ws_connections: AtomicU64::new(0),
            ws_connects_total: AtomicU64::new(0),
            persist_failures_total: AtomicU64::new(0),
            event_drops_total: AtomicU64::new(0),
            latency_buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            latency_infinite: AtomicU64::new(0),
        })
    }

    /// Seconds since process start, for uptime alerting and dashboards.
    pub fn uptime_seconds(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    pub fn record_request(&self, status: u16, latency: Duration) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        if status >= 500 {
            self.errors_total.fetch_add(1, Ordering::Relaxed);
        }
        let seconds = latency.as_secs_f64();
        for (index, bound) in LATENCY_BUCKETS.iter().enumerate() {
            if seconds <= *bound {
                self.latency_buckets[index].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        self.latency_infinite.fetch_add(1, Ordering::Relaxed);
    }

    pub fn ws_opened(&self) {
        self.ws_connections.fetch_add(1, Ordering::Relaxed);
        self.ws_connects_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn ws_closed(&self) {
        self.ws_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_persist_failure(&self) {
        self.persist_failures_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_event_drop(&self) {
        self.event_drops_total.fetch_add(1, Ordering::Relaxed);
    }

    fn count(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }

    fn gauge(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }

    /// Render the Prometheus text exposition format (version 0.0.4).
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(2048);
        let mut metric = |kind: &str, name: &str, help: &str, value: u64| {
            out.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}\n"
            ));
        };
        metric(
            "counter",
            "chao_http_requests_total",
            "Total HTTP requests served.",
            Self::count(&self.requests_total),
        );
        metric(
            "counter",
            "chao_http_errors_total",
            "HTTP responses with status >= 500.",
            Self::count(&self.errors_total),
        );
        metric(
            "counter",
            "chao_ws_connects_total",
            "Total WebSocket upgrades accepted.",
            Self::count(&self.ws_connects_total),
        );
        metric(
            "gauge",
            "chao_ws_connections",
            "Currently open WebSocket connections.",
            Self::gauge(&self.ws_connections),
        );
        metric(
            "counter",
            "chao_persist_failures_total",
            "Failed profile persistence attempts (disk or DB saturation signal).",
            Self::count(&self.persist_failures_total),
        );
        metric(
            "counter",
            "chao_event_drops_total",
            "Events dropped when the broadcast queue was full (Redis/pubsub proxy).",
            Self::count(&self.event_drops_total),
        );
        out.push_str("# HELP chao_http_request_duration_seconds HTTP request latency.\n# TYPE chao_http_request_duration_seconds histogram\n");
        let mut cumulative = 0u64;
        for (bound, bucket) in LATENCY_BUCKETS.iter().zip(&self.latency_buckets) {
            cumulative += Self::count(bucket);
            out.push_str(&format!(
                "chao_http_request_duration_seconds_bucket{{le=\"{bound}\"}} {cumulative}\n"
            ));
        }
        cumulative += Self::count(&self.latency_infinite);
        out.push_str(&format!(
            "chao_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {cumulative}\n"
        ));
        out.push_str(&format!(
            "chao_http_request_duration_seconds_sum 0\nchao_http_request_duration_seconds_count {cumulative}\n"
        ));
        out
    }
}
