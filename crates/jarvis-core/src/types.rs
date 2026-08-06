use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Deny,
    Observe,
    Confirm,
    Auto,
}

impl Default for TrustLevel {
    fn default() -> Self {
        Self::Confirm
    }
}

impl fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deny => write!(f, "deny"),
            Self::Observe => write!(f, "observe"),
            Self::Confirm => write!(f, "confirm"),
            Self::Auto => write!(f, "auto"),
        }
    }
}

impl FromStr for TrustLevel {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "deny" => Ok(Self::Deny),
            "observe" => Ok(Self::Observe),
            "confirm" => Ok(Self::Confirm),
            "auto" => Ok(Self::Auto),
            _ => Err(format!("unknown trust level: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl fmt::Display for RunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepRunStatus {
    Pending,
    AwaitingConfirm,
    Running,
    Skipped,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerKind {
    Cli,
    Voice,
    Schedule,
    Suggest,
    Http,
    Messaging,
    Phrase,
}

impl fmt::Display for TriggerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli => write!(f, "cli"),
            Self::Voice => write!(f, "voice"),
            Self::Schedule => write!(f, "schedule"),
            Self::Suggest => write!(f, "suggest"),
            Self::Http => write!(f, "http"),
            Self::Messaging => write!(f, "messaging"),
            Self::Phrase => write!(f, "phrase"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedEvent {
    pub ts: String,
    pub source: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub playbook_id: String,
    pub playbook_ver: i64,
    pub status: String,
    pub trigger: String,
    pub phrase: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRecord {
    pub id: String,
    pub fingerprint: String,
    pub event_ids: Vec<i64>,
    pub suggested_name: Option<String>,
    pub confidence: f64,
    pub status: String,
    pub command_sequence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustGrant {
    pub playbook_id: String,
    pub step_id: String,
    pub level: TrustLevel,
    pub granted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmRequest {
    pub run_id: String,
    pub step_id: String,
    pub step_name: String,
    pub trust: TrustLevel,
    pub summary: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmResponse {
    Approve,
    Skip,
    Abort,
}
