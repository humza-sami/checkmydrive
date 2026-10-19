//! Wire types. Must match `docs/CONTRACT.md` exactly (snake_case JSON).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub type Params = BTreeMap<String, Value>;
pub type Metrics = BTreeMap<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveKind {
    Hdd,
    Ssd,
    Nvme,
    Usb,
    Sd,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    pub id: String,
    pub path: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub vendor: Option<String>,
    pub capacity_bytes: u64,
    pub kind: DriveKind,
    pub bus: Option<String>,
    pub rotation_rpm: Option<u32>,
    pub removable: bool,
    pub is_system_disk: bool,
    pub mount_points: Vec<String>,
    pub smart_supported: Option<bool>,
    pub power_on_hours: Option<u64>,
    pub manufacture_date: Option<String>,
}

impl DriveInfo {
    /// True when the drive spins (so HDD latency thresholds apply).
    pub fn is_rotational(&self) -> bool {
        self.kind == DriveKind::Hdd || self.rotation_rpm.map(|r| r > 0).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestId {
    DriveInfo,
    SmartHealth,
    SelftestShort,
    SurfaceRead,
    WriteVerify,
    CapacityCheck,
    Stability,
}

impl TestId {
    pub const ALL: [TestId; 7] = [
        TestId::DriveInfo,
        TestId::SmartHealth,
        TestId::SelftestShort,
        TestId::SurfaceRead,
        TestId::WriteVerify,
        TestId::CapacityCheck,
        TestId::Stability,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Params>,
}

impl Issue {
    pub fn new(severity: Severity, code: &str) -> Self {
        Issue { severity, code: code.to_string(), params: None }
    }
    pub fn info(code: &str) -> Self {
        Self::new(Severity::Info, code)
    }
    pub fn warn(code: &str) -> Self {
        Self::new(Severity::Warn, code)
    }
    pub fn critical(code: &str) -> Self {
        Self::new(Severity::Critical, code)
    }
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.params.get_or_insert_with(Params::new).insert(key.to_string(), value.into());
        self
    }
    /// Numeric magnitude hint used by scoring (`count` param), if any.
    pub fn magnitude(&self) -> f64 {
        self.params
            .as_ref()
            .and_then(|p| p.get("count"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub id: TestId,
    pub status: Status,
    pub score: Option<u8>,
    pub duration_ms: u64,
    pub metrics: Metrics,
    pub issues: Vec<Issue>,
}

impl TestResult {
    pub fn skipped(id: TestId, issues: Vec<Issue>) -> Self {
        TestResult { id, status: Status::Skipped, score: None, duration_ms: 0, metrics: Metrics::new(), issues }
    }
    /// Build a result, deriving status and score from the issues.
    pub fn from_issues(id: TestId, duration_ms: u64, metrics: Metrics, issues: Vec<Issue>) -> Self {
        let status = crate::score::status_from_issues(&issues);
        let score = crate::score::test_score(&issues);
        TestResult { id, status, score: Some(score), duration_ms, metrics, issues }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Buy,
    Caution,
    Avoid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuitLevel {
    Good,
    Caution,
    No,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suitability {
    pub ok: bool,
    pub level: SuitLevel,
    pub reason_code: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Params>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuitabilityReport {
    pub os_install: Suitability,
    pub long_term_storage: Suitability,
    pub daily_use: Suitability,
    pub safe_fill_bytes: u64,
    pub safe_fill_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Age {
    pub power_on_hours: Option<u64>,
    pub power_on_years: Option<f64>,
    pub manufacture_date: Option<String>,
    pub power_cycles: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Safe,
    Wipe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub schema_version: u32,
    pub engine_version: String,
    pub drive: DriveInfo,
    pub mode: Mode,
    pub started_at: String,
    pub duration_ms: u64,
    pub tests: Vec<TestResult>,
    pub score: u8,
    pub verdict: Verdict,
    pub issues: Vec<Issue>,
    pub age: Age,
    pub suitability: SuitabilityReport,
    /// Lowest hard cap that applies to the score (None when no cap applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_cap: Option<u8>,
    /// i18n codes (`score.reason.*`) explaining the score, worst first; empty for a perfect score.
    #[serde(default)]
    pub score_reasons: Vec<String>,
}

/// `phase` of a progress event: a test id, `prepare` or `finish`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepare,
    DriveInfo,
    SmartHealth,
    SelftestShort,
    SurfaceRead,
    WriteVerify,
    CapacityCheck,
    Stability,
    Finish,
}

impl From<TestId> for Phase {
    fn from(t: TestId) -> Phase {
        match t {
            TestId::DriveInfo => Phase::DriveInfo,
            TestId::SmartHealth => Phase::SmartHealth,
            TestId::SelftestShort => Phase::SelftestShort,
            TestId::SurfaceRead => Phase::SurfaceRead,
            TestId::WriteVerify => Phase::WriteVerify,
            TestId::CapacityCheck => Phase::CapacityCheck,
            TestId::Stability => Phase::Stability,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    #[serde(rename = "type")]
    pub kind: String, // always "progress"
    pub phase: Phase,
    pub percent: f64,
    pub phase_percent: f64,
    pub message_code: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Params>,
}

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
