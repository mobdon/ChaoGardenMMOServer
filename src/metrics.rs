//! Bounded, dependency-free process metrics. Never label series with identities.
use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const LATENCY_BUCKETS: &[f64] = &[
    0.0001, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10.,
];

#[derive(Debug, Default)]
pub struct Histogram {
    // Snapshot the related fields together; independent atomic loads can emit
    // buckets, count and sum from different observations during a scrape.
    snapshot_lock: Mutex<()>,
    buckets: [AtomicU64; LATENCY_BUCKETS.len()],
    count: AtomicU64,
    microseconds: AtomicU64,
}
impl Histogram {
    pub fn record(&self, duration: Duration) {
        let _guard = self.snapshot_lock.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(index) = LATENCY_BUCKETS
            .iter()
            .position(|bound| duration.as_secs_f64() <= *bound)
        {
            self.buckets[index].fetch_add(1, Ordering::Relaxed);
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        self.microseconds.fetch_add(
            duration.as_micros().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
    }
    fn render(&self, out: &mut String, name: &str, help: &str) {
        let (buckets, count, microseconds) = {
            let _guard = self.snapshot_lock.lock().unwrap_or_else(|p| p.into_inner());
            (
                std::array::from_fn::<_, { LATENCY_BUCKETS.len() }, _>(|index| {
                    self.buckets[index].load(Ordering::Relaxed)
                }),
                self.count.load(Ordering::Relaxed),
                self.microseconds.load(Ordering::Relaxed),
            )
        };
        // Keep text formatting outside the histogram lock.
        let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} histogram");
        let mut cumulative = 0;
        for (bound, bucket) in LATENCY_BUCKETS.iter().zip(buckets) {
            cumulative += bucket;
            let _ = writeln!(out, "{name}_bucket{{le=\"{bound}\"}} {cumulative}");
        }
        let sum = microseconds as f64 / 1_000_000.;
        let _ = writeln!(
            out,
            "{name}_bucket{{le=\"+Inf\"}} {count}\n{name}_sum {sum}\n{name}_count {count}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Barrier, atomic::AtomicBool};

    #[test]
    fn concurrent_recording_and_scraping_keep_histogram_fields_coherent() {
        let histogram = Arc::new(Histogram::default());
        let start = Arc::new(Barrier::new(5));
        let stop = Arc::new(AtomicBool::new(false));
        let writers: Vec<_> = (0..4)
            .map(|_| {
                let histogram = histogram.clone();
                let start = start.clone();
                let stop = stop.clone();
                std::thread::spawn(move || {
                    start.wait();
                    while !stop.load(Ordering::Relaxed) {
                        histogram.record(Duration::from_millis(1));
                    }
                })
            })
            .collect();
        start.wait();
        let mut observations = Vec::with_capacity(1000);
        for _ in 0..1000 {
            let mut rendered = String::new();
            histogram.render(&mut rendered, "test_latency_seconds", "Test.");
            observations.push(rendered);
        }
        stop.store(true, Ordering::Relaxed);
        for writer in writers {
            writer.join().unwrap();
        }

        let mut previous_count = 0;
        for rendered in observations {
            let mut buckets = vec![];
            let mut count = None;
            let mut sum = None;
            for line in rendered.lines() {
                if line.starts_with('#') {
                    continue;
                }
                let (key, value) = line.rsplit_once(' ').unwrap();
                let value: f64 = value.parse().unwrap();
                if key.contains("_bucket{") {
                    buckets.push(value as u64);
                } else if key.ends_with("_count") {
                    count = Some(value as u64);
                } else if key.ends_with("_sum") {
                    sum = Some(value);
                }
            }
            let count = count.unwrap();
            assert!(count >= previous_count);
            previous_count = count;
            assert!(buckets.windows(2).all(|pair| pair[0] <= pair[1]));
            assert!(buckets.iter().all(|bucket| *bucket <= count));
            assert_eq!(buckets.last().copied(), Some(count));
            // Every observation is exactly 1 ms: count, finite buckets and sum
            // must refer to the same completed set of records.
            assert_eq!(buckets[2], count);
            assert!((sum.unwrap() - count as f64 / 1000.).abs() < 1e-9);
        }
        assert!(
            previous_count > 0,
            "The concurrent writers must have recorded samples"
        );
    }
}

#[derive(Debug)]
pub struct Metrics {
    started: Instant,
    requests_total: AtomicU64,
    errors_total: AtomicU64,
    ws_connections: AtomicU64,
    ws_connects_total: AtomicU64,
    reconnects_total: AtomicU64,
    admission_rejected_total: AtomicU64,
    persist_failures_total: AtomicU64,
    event_lag_total: AtomicU64,
    slow_clients_total: AtomicU64,
    oversized_snapshots_total: AtomicU64,
    input_rate_disconnects_total: AtomicU64,
    snapshot_bytes_total: AtomicU64,
    snapshots_total: AtomicU64,
    event_queue_depth: AtomicU64,
    event_queue_high_water: AtomicU64,
    residents: AtomicU64,
    pub http: Histogram,
    pub tick: Histogram,
    pub tick_lock_wait: Histogram,
    pub resident_tick: Histogram,
    pub snapshot: Histogram,
    pub save: Histogram,
    pub send: Histogram,
}
impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            requests_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            ws_connections: AtomicU64::new(0),
            ws_connects_total: AtomicU64::new(0),
            reconnects_total: AtomicU64::new(0),
            admission_rejected_total: AtomicU64::new(0),
            persist_failures_total: AtomicU64::new(0),
            event_lag_total: AtomicU64::new(0),
            slow_clients_total: AtomicU64::new(0),
            oversized_snapshots_total: AtomicU64::new(0),
            input_rate_disconnects_total: AtomicU64::new(0),
            snapshot_bytes_total: AtomicU64::new(0),
            snapshots_total: AtomicU64::new(0),
            event_queue_depth: AtomicU64::new(0),
            event_queue_high_water: AtomicU64::new(0),
            residents: AtomicU64::new(0),
            http: Histogram::default(),
            tick: Histogram::default(),
            tick_lock_wait: Histogram::default(),
            resident_tick: Histogram::default(),
            snapshot: Histogram::default(),
            save: Histogram::default(),
            send: Histogram::default(),
        })
    }
    pub fn uptime_seconds(&self) -> u64 {
        self.started.elapsed().as_secs()
    }
    pub fn record_request(&self, status: u16, latency: Duration) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        if status >= 500 {
            self.errors_total.fetch_add(1, Ordering::Relaxed);
        }
        self.http.record(latency);
    }
    pub fn ws_opened(&self, reconnect: bool) {
        self.ws_connections.fetch_add(1, Ordering::Relaxed);
        self.ws_connects_total.fetch_add(1, Ordering::Relaxed);
        if reconnect {
            self.reconnects_total.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn ws_closed(&self) {
        self.ws_connections.fetch_sub(1, Ordering::Relaxed);
    }
    pub fn record_persist_failure(&self) {
        self.persist_failures_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_event_lag(&self, count: u64) {
        self.event_lag_total.fetch_add(count, Ordering::Relaxed);
    }
    pub fn record_slow_client(&self) {
        self.slow_clients_total.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_oversized_snapshot(&self) {
        self.oversized_snapshots_total
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_rate_disconnect(&self) {
        self.input_rate_disconnects_total
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_admission_rejected(&self) {
        self.admission_rejected_total
            .fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_snapshot_sent(&self, bytes: usize) {
        self.snapshots_total.fetch_add(1, Ordering::Relaxed);
        self.snapshot_bytes_total
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
    pub fn set_queue_depth(&self, depth: usize) {
        self.event_queue_depth
            .store(depth as u64, Ordering::Relaxed);
        self.event_queue_high_water
            .fetch_max(depth as u64, Ordering::Relaxed);
    }
    pub fn set_residents(&self, count: usize) {
        self.residents.store(count as u64, Ordering::Relaxed);
    }
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(8192);
        for (name, help, counter) in [
            (
                "chao_http_requests_total",
                "Total HTTP requests served.",
                &self.requests_total,
            ),
            (
                "chao_http_errors_total",
                "HTTP responses with status at least 500.",
                &self.errors_total,
            ),
            (
                "chao_ws_connects_total",
                "WebSocket connections opened.",
                &self.ws_connects_total,
            ),
            (
                "chao_reconnects_total",
                "Opened sockets for a profile previously connected in this process.",
                &self.reconnects_total,
            ),
            (
                "chao_admission_rejected_total",
                "Connections or new profiles rejected at configured capacity.",
                &self.admission_rejected_total,
            ),
            (
                "chao_persist_failures_total",
                "Failed dirty-profile atomic save attempts.",
                &self.persist_failures_total,
            ),
            (
                "chao_event_lag_total",
                "Broadcast entries missed by lagging peers; those peers disconnect.",
                &self.event_lag_total,
            ),
            (
                "chao_slow_clients_total",
                "Socket sends exceeding the configured deadline.",
                &self.slow_clients_total,
            ),
            (
                "chao_oversized_snapshots_total",
                "Snapshots exceeding byte bound; peer disconnected.",
                &self.oversized_snapshots_total,
            ),
            (
                "chao_input_rate_disconnects_total",
                "Peers exceeding 80 incoming messages per second.",
                &self.input_rate_disconnects_total,
            ),
            (
                "chao_snapshot_bytes_total",
                "Snapshot UTF-8 payload bytes successfully sent; excludes WS/TLS overhead.",
                &self.snapshot_bytes_total,
            ),
            (
                "chao_snapshots_total",
                "Snapshot frames successfully sent.",
                &self.snapshots_total,
            ),
        ] {
            let _ = writeln!(
                out,
                "# HELP {name} {help}\n# TYPE {name} counter\n{name} {}",
                counter.load(Ordering::Relaxed)
            );
        }
        for (name, help, value) in [
            (
                "chao_ws_connections",
                "Currently open WebSocket connections.",
                self.ws_connections.load(Ordering::Relaxed),
            ),
            (
                "chao_event_queue_depth",
                "Last observed retained broadcast event entries.",
                self.event_queue_depth.load(Ordering::Relaxed),
            ),
            (
                "chao_event_queue_high_water",
                "Maximum observed retained broadcast event entries.",
                self.event_queue_high_water.load(Ordering::Relaxed),
            ),
            (
                "chao_residents",
                "Residents simulated in the last tick.",
                self.residents.load(Ordering::Relaxed),
            ),
            (
                "chao_uptime_seconds",
                "Process uptime.",
                self.uptime_seconds(),
            ),
        ] {
            let _ = writeln!(
                out,
                "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}"
            );
        }
        for (name, help, histogram) in [
            (
                "chao_http_request_duration_seconds",
                "HTTP handler latency.",
                &self.http,
            ),
            (
                "chao_tick_duration_seconds",
                "Simulation work including pending reward save; excludes lock wait.",
                &self.tick,
            ),
            (
                "chao_tick_lock_wait_seconds",
                "Simulation wait for authoritative game mutex.",
                &self.tick_lock_wait,
            ),
            (
                "chao_resident_tick_duration_seconds",
                "Time advancing resident AI per simulation tick.",
                &self.resident_tick,
            ),
            (
                "chao_snapshot_duration_seconds",
                "Snapshot build and JSON encoding including lock wait.",
                &self.snapshot,
            ),
            (
                "chao_save_duration_seconds",
                "Dirty profile export, encode, fsync and atomic replacement.",
                &self.save,
            ),
            (
                "chao_send_duration_seconds",
                "Socket send completion or failure latency.",
                &self.send,
            ),
        ] {
            histogram.render(&mut out, name, help);
        }
        out
    }
}
