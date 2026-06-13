//! Canonical string values for research job types and lifecycle statuses.
//!
//! These back the control-flow matchers that gate report regeneration and
//! queue transitions, so the matched value sets live in one place. The values
//! must stay byte-identical to the persisted database strings.

/// Backtest job type that replays one fixed parameter set.
pub(crate) const JOB_TYPE_CURRENT_PARAMS: &str = "current_params";

/// Sweep job type that evaluates a grid of parameter sets.
pub(crate) const JOB_TYPE_SWEEP: &str = "sweep";

/// Terminal status for a successfully finished job, transfer, or step.
pub(crate) const STATUS_COMPLETED: &str = "completed";

/// Terminal status for a failed job, transfer, or step.
pub(crate) const STATUS_FAILED: &str = "failed";

/// Terminal status for an operator-cancelled job, transfer, or step.
pub(crate) const STATUS_CANCELLED: &str = "cancelled";

/// Status for a job or step blocked pending operator review.
pub(crate) const STATUS_BLOCKED: &str = "blocked";

/// Status for a retryable job or step awaiting another attempt.
pub(crate) const STATUS_RETRYABLE: &str = "retryable";
