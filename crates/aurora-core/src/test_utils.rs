//! Shared test utilities for Aurora Note crates.
//!
//! Exposed under `#[cfg(any(test, feature = "test-utils"))]` so downstream crates
//! can reuse helpers when running tests or when the `test-utils` feature is enabled.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

// ============================================================================
// MockClock — deterministic time provider for tests
// ============================================================================

/// A deterministic clock that returns monotonically increasing timestamps.
/// Replaces `chrono::Utc::now()` in tests for reproducible behaviour.
#[derive(Debug, Clone)]
pub struct MockClock {
    epoch_ms: Arc<AtomicU64>,
    step_ms: u64,
}

impl MockClock {
    /// Create a new `MockClock` starting at the given epoch (ms) and stepping
    /// by `step_ms` on every call to `now()`.
    pub fn new(start_ms: u64, step_ms: u64) -> Self {
        Self {
            epoch_ms: Arc::new(AtomicU64::new(start_ms)),
            step_ms,
        }
    }

    /// Default clock starting at `1704067200000` (2024-01-01T00:00:00Z) with
    /// a 1 ms step.
    pub fn default() -> Self {
        Self::new(1_704_067_200_000, 1)
    }

    /// Advance the clock and return the new timestamp in milliseconds.
    pub fn now_ms(&self) -> u64 {
        self.epoch_ms.fetch_add(self.step_ms, Ordering::SeqCst)
    }

    /// Return a `chrono::DateTime<chrono::Utc>` from the current clock value.
    pub fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
        let ms = self.now_ms();
        chrono::DateTime::from_timestamp_millis(ms as i64)
            .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH)
    }

    /// Manually advance the clock by `ms` milliseconds.
    pub fn advance(&self, ms: u64) {
        self.epoch_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::default()
    }
}

// ============================================================================
// TempWorkspace — RAII temporary directory for test workspaces
// ============================================================================

/// RAII temporary directory that is recursively deleted when dropped.
#[derive(Debug)]
pub struct TempWorkspace {
    pub path: PathBuf,
}

impl TempWorkspace {
    /// Create a new temporary directory with a unique name under the system
    /// temp directory.
    pub fn new() -> Self {
        let uuid = uuid::Uuid::new_v4().to_string();
        let path = std::env::temp_dir().join(format!("aurora-test-{}", uuid));
        fs::create_dir_all(&path).expect("failed to create temp directory");
        Self { path }
    }

    /// Return the path as a string slice.
    pub fn path_str(&self) -> &str {
        self.path.to_str().unwrap_or("")
    }

    /// Join a relative sub-path.
    pub fn join<P: AsRef<Path>>(&self, sub: P) -> PathBuf {
        self.path.join(sub)
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl Default for TempWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TestEventBus — in-memory event bus for integration tests between modules
// ============================================================================

/// A synchronous, in-memory event bus intended for cross-module integration
/// tests.  Events are collected in a `Vec` so assertions can inspect the full
/// history.
#[derive(Debug, Clone)]
pub struct TestEventBus<E: Clone + Send + 'static> {
    history: Arc<Mutex<Vec<E>>>,
}

impl<E: Clone + Send + 'static> TestEventBus<E> {
    pub fn new() -> Self {
        Self {
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Publish (record) an event.
    pub fn publish(&self, event: E) {
        let mut h = self.history.lock().unwrap();
        h.push(event);
    }

    /// Return a clone of the full event history.
    pub fn history(&self) -> Vec<E> {
        self.history.lock().unwrap().clone()
    }

    /// Return the number of recorded events.
    pub fn len(&self) -> usize {
        self.history.lock().unwrap().len()
    }

    /// Return `true` if no events have been recorded.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear the event history.
    pub fn clear(&self) {
        self.history.lock().unwrap().clear();
    }

    /// Return the last recorded event, if any.
    pub fn last(&self) -> Option<E> {
        self.history.lock().unwrap().last().cloned()
    }
}

impl<E: Clone + Send + 'static> Default for TestEventBus<E> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// SnapshotAsserter — helper to assert JSON snapshot equality
// ============================================================================

/// Lightweight snapshot assertion helper.
///
/// On mismatch it panics with a pretty-printed diff of the JSON
/// representation.
pub struct SnapshotAsserter;

impl SnapshotAsserter {
    /// Assert that `value` serializes to the expected JSON string.
    pub fn assert_eq<T: Serialize>(value: &T, expected_json: &str) {
        let actual = serde_json::to_string_pretty(value).expect("serialization failed");
        let expected: serde_json::Value =
            serde_json::from_str(expected_json).expect("expected_json is not valid JSON");
        let expected_pretty = serde_json::to_string_pretty(&expected).unwrap();

        if actual != expected_pretty {
            panic!(
                "Snapshot mismatch.\nExpected:\n{}\n\nActual:\n{}\n",
                expected_pretty, actual
            );
        }
    }

    /// Assert that two serializable values produce the same JSON.
    pub fn assert_eq_values<T: Serialize, U: Serialize>(a: &T, b: &U) {
        let json_a = serde_json::to_string(a).expect("serialization failed");
        let json_b = serde_json::to_string(b).expect("serialization failed");
        if json_a != json_b {
            let pretty_a = serde_json::to_string_pretty(
                &serde_json::from_str::<serde_json::Value>(&json_a).unwrap(),
            )
            .unwrap();
            let pretty_b = serde_json::to_string_pretty(
                &serde_json::from_str::<serde_json::Value>(&json_b).unwrap(),
            )
            .unwrap();
            panic!(
                "Snapshot mismatch.\nA:\n{}\n\nB:\n{}\n",
                pretty_a, pretty_b
            );
        }
    }
}

// ============================================================================
// SimpleBenchmark — minimal benchmark runner (no Criterion dependency)
// ============================================================================

/// Timing statistics produced by [`SimpleBenchmark`].
#[derive(Debug, Clone, Copy)]
pub struct BenchStats {
    pub iterations: usize,
    pub mean_ms: f64,
    pub median_ms: f64,
    pub stddev_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

/// Minimal benchmark runner that measures wall-clock time over `N`
/// iterations and reports basic statistics.
pub struct SimpleBenchmark {
    name: String,
    iterations: usize,
    times_nanos: Vec<u128>,
}

impl SimpleBenchmark {
    pub fn new(name: impl Into<String>, iterations: usize) -> Self {
        Self {
            name: name.into(),
            iterations,
            times_nanos: Vec::with_capacity(iterations),
        }
    }

    /// Run `f` for `iterations` times, measuring each invocation.
    pub fn run<F, T>(&mut self, mut f: F) -> BenchStats
    where
        F: FnMut() -> T,
    {
        self.times_nanos.clear();
        for _ in 0..self.iterations {
            let start = Instant::now();
            let _ = f();
            let elapsed = start.elapsed().as_nanos();
            self.times_nanos.push(elapsed);
        }
        self.stats()
    }

    /// Run `f` for `iterations` times with a per-iteration setup step.
    pub fn run_with_setup<F, S, T>(&mut self, mut setup: S, mut f: F) -> BenchStats
    where
        S: FnMut(usize),
        F: FnMut() -> T,
    {
        self.times_nanos.clear();
        for i in 0..self.iterations {
            setup(i);
            let start = Instant::now();
            let _ = f();
            let elapsed = start.elapsed().as_nanos();
            self.times_nanos.push(elapsed);
        }
        self.stats()
    }

    /// Compute statistics from the collected timings.
    pub fn stats(&self) -> BenchStats {
        let mut sorted = self.times_nanos.clone();
        sorted.sort_unstable();
        let len = sorted.len() as f64;
        let sum: u128 = sorted.iter().sum();
        let mean = sum as f64 / len;
        let median = if sorted.len() % 2 == 0 {
            let mid = sorted.len() / 2;
            (sorted[mid - 1] as f64 + sorted[mid] as f64) / 2.0
        } else {
            sorted[sorted.len() / 2] as f64
        };
        let variance = sorted.iter().map(|&t| {
            let diff = t as f64 - mean;
            diff * diff
        }).sum::<f64>() / len;
        let stddev = variance.sqrt();

        BenchStats {
            iterations: self.times_nanos.len(),
            mean_ms: mean / 1_000_000.0,
            median_ms: median / 1_000_000.0,
            stddev_ms: stddev / 1_000_000.0,
            min_ms: *sorted.first().unwrap_or(&0) as f64 / 1_000_000.0,
            max_ms: *sorted.last().unwrap_or(&0) as f64 / 1_000_000.0,
        }
    }

    /// Print human-readable benchmark results to stdout.
    pub fn report(&self) {
        let s = self.stats();
        println!(
            "Benchmark '{}' ({} iterations): mean={:.3}ms median={:.3}ms stddev={:.3}ms min={:.3}ms max={:.3}ms",
            self.name, s.iterations, s.mean_ms, s.median_ms, s.stddev_ms, s.min_ms, s.max_ms
        );
    }

    /// Assert that the mean execution time is below `threshold_ms`.
    pub fn assert_mean_below(&self, threshold_ms: f64) {
        let s = self.stats();
        assert!(
            s.mean_ms < threshold_ms,
            "Benchmark '{}' mean {:.3}ms >= threshold {:.3}ms",
            self.name,
            s.mean_ms,
            threshold_ms
        );
    }
}

// ============================================================================
// CoverageTracer — counts which code paths were exercised
// ============================================================================

/// Lightweight instrumentation helper that counts how many times named code
/// paths were hit during a test suite.
#[derive(Debug, Clone)]
pub struct CoverageTracer {
    counters: Arc<Mutex<HashMap<String, usize>>>,
}

impl CoverageTracer {
    pub fn new() -> Self {
        Self {
            counters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record that `path` was exercised.
    pub fn hit(&self, path: impl Into<String>) {
        let mut c = self.counters.lock().unwrap();
        *c.entry(path.into()).or_insert(0) += 1;
    }

    /// Return the hit count for `path`.
    pub fn count(&self, path: &str) -> usize {
        self.counters.lock().unwrap().get(path).copied().unwrap_or(0)
    }

    /// Return `true` if `path` was exercised at least once.
    pub fn covered(&self, path: &str) -> bool {
        self.count(path) > 0
    }

    /// Return the percentage of provided paths that were covered.
    pub fn coverage_percent(&self, paths: &[&str]) -> f64 {
        if paths.is_empty() {
            return 100.0;
        }
        let covered = paths.iter().filter(|&&p| self.covered(p)).count();
        (covered as f64 / paths.len() as f64) * 100.0
    }

    /// Reset all counters.
    pub fn reset(&self) {
        self.counters.lock().unwrap().clear();
    }

    /// Return a snapshot of all counters.
    pub fn snapshot(&self) -> HashMap<String, usize> {
        self.counters.lock().unwrap().clone()
    }
}

impl Default for CoverageTracer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TestMatrix — records which tests cover which modules (manual registration)
// ============================================================================

/// Manual test-to-module coverage matrix.
///
/// Tests register themselves together with the modules they intend to cover,
/// producing a matrix that can be inspected or asserted at the end of a test
/// run.
#[derive(Debug, Clone)]
pub struct TestMatrix {
    entries: Arc<Mutex<Vec<TestMatrixEntry>>>,
}

#[derive(Debug, Clone)]
pub struct TestMatrixEntry {
    pub test_name: String,
    pub module_path: String,
}

impl TestMatrix {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register that `test_name` covers `module_path`.
    pub fn register(&self, test_name: impl Into<String>, module_path: impl Into<String>) {
        self.entries.lock().unwrap().push(TestMatrixEntry {
            test_name: test_name.into(),
            module_path: module_path.into(),
        });
    }

    /// Return the full list of registered entries.
    pub fn entries(&self) -> Vec<TestMatrixEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// Return the set of distinct modules that have been registered.
    pub fn covered_modules(&self) -> Vec<String> {
        let mut mods: Vec<String> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.module_path.clone())
            .collect();
        mods.sort_unstable();
        mods.dedup();
        mods
    }

    /// Return the set of test names that claim to cover `module_path`.
    pub fn tests_for_module(&self, module_path: &str) -> Vec<String> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.module_path == module_path)
            .map(|e| e.test_name.clone())
            .collect()
    }

    /// Assert that every module in `required_modules` has at least one test
    /// registered.
    pub fn assert_all_modules_covered(&self, required_modules: &[&str]) {
        let covered = self.covered_modules();
        for module in required_modules {
            assert!(
                covered.contains(&module.to_string()),
                "Module '{}' has no registered tests in the TestMatrix",
                module
            );
        }
    }
}

impl Default for TestMatrix {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Unit tests for the helpers themselves
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_clock_advances() {
        let clock = MockClock::new(1000, 5);
        assert_eq!(clock.now_ms(), 1000);
        assert_eq!(clock.now_ms(), 1005);
        assert_eq!(clock.now_ms(), 1010);
    }

    #[test]
    fn test_mock_clock_utc() {
        let clock = MockClock::new(0, 0);
        let dt = clock.now_utc();
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn test_temp_workspace_lifecycle() {
        let ws = TempWorkspace::new();
        assert!(ws.path.exists());
        let file = ws.join("test.txt");
        fs::write(&file, "hello").unwrap();
        assert!(file.exists());
        drop(ws);
        assert!(!file.exists());
    }

    #[test]
    fn test_test_event_bus() {
        let bus = TestEventBus::<String>::new();
        bus.publish("a".to_string());
        bus.publish("b".to_string());
        assert_eq!(bus.len(), 2);
        assert_eq!(bus.last(), Some("b".to_string()));
        bus.clear();
        assert!(bus.is_empty());
    }

    #[test]
    fn test_snapshot_asserter() {
        SnapshotAsserter::assert_eq(&serde_json::json!({"x": 1}), r#"{"x": 1}"#);
    }

    #[test]
    fn test_simple_benchmark_stats() {
        let mut bench = SimpleBenchmark::new("noop", 5);
        bench.run(|| ());
        let stats = bench.stats();
        assert_eq!(stats.iterations, 5);
        assert!(stats.mean_ms >= 0.0);
    }

    #[test]
    fn test_coverage_tracer() {
        let tracer = CoverageTracer::new();
        tracer.hit("path_a");
        tracer.hit("path_a");
        tracer.hit("path_b");
        assert_eq!(tracer.count("path_a"), 2);
        assert!(tracer.covered("path_b"));
        assert!(!tracer.covered("path_c"));
        assert_eq!(tracer.coverage_percent(&["path_a", "path_b", "path_c"]), 66.66666666666667);
    }

    #[test]
    fn test_test_matrix() {
        let matrix = TestMatrix::new();
        matrix.register("test_foo", "module_a");
        matrix.register("test_bar", "module_a");
        matrix.register("test_baz", "module_b");
        assert_eq!(matrix.covered_modules(), vec!["module_a", "module_b"]);
        assert_eq!(matrix.tests_for_module("module_a").len(), 2);
        matrix.assert_all_modules_covered(&["module_a", "module_b"]);
    }
}
