//! Custom MMO idle training. These are not native Tiny Chao Garden rules.
//!
//! All timestamps and policy values come from the authoritative server.
//! Persist a settlement and its canonical-stat award in one transaction.
//! This pure core alone cannot supply database or account authorization.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_DURATION_MS: u64 = 8 * 60 * 60 * 1000;
/// MMO fee, not a cartridge price: a flat rate per minute for each skill band.
pub fn ring_rate(skill_level: u16) -> u32 {
    1 + u32::from(skill_level.min(99) / 25)
}

pub fn ring_quote(duration_ms: u64, rate: u32) -> Option<u32> {
    if !(60_000..=MAX_DURATION_MS).contains(&duration_ms)
        || !duration_ms.is_multiple_of(60_000)
        || !(1..=4).contains(&rate)
    {
        return None;
    }
    Some((duration_ms / 60_000) as u32 * rate)
}
pub const MAX_AWARD_POINTS: u32 = 10_000;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stat {
    Swim,
    Fly,
    Run,
    Power,
    Stamina,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrainingError {
    InvalidTarget,
    InvalidPolicy,
    InvalidDuration,
    ActiveJob,
    MissingJob,
    WrongJob,
    JobClosed,
    ClockMovedBackwards,
    TimestampOverflow,
    IdExhausted,
    NotReady { remaining_ms: u64 },
    ReadyToSettle,
    InvalidStoredState,
    StoredTargetMismatch,
    Serialization(String),
}
impl fmt::Display for TrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TrainingError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrainingPolicy {
    tick_ms: u64,
    points_per_tick: u32,
    max_duration_ms: u64,
    max_points: u32,
}

impl Default for TrainingPolicy {
    fn default() -> Self {
        Self {
            tick_ms: 60_000,
            points_per_tick: 1,
            max_duration_ms: MAX_DURATION_MS,
            max_points: 480,
        }
    }
}

impl TrainingPolicy {
    /// Server configuration only; never deserialize a policy from a client
    /// action. Jobs retain the validated policy they started with.
    pub fn from_server_config(
        tick_ms: u64,
        points_per_tick: u32,
        max_duration_ms: u64,
        max_points: u32,
    ) -> Result<Self, TrainingError> {
        let policy = Self {
            tick_ms,
            points_per_tick,
            max_duration_ms,
            max_points,
        };
        policy.validate()?;
        Ok(policy)
    }

    fn validate(&self) -> Result<(), TrainingError> {
        if self.tick_ms == 0
            || self.tick_ms > self.max_duration_ms
            || self.max_duration_ms > MAX_DURATION_MS
            || self.points_per_tick == 0
            || self.points_per_tick > 1_000
            || self.max_points == 0
            || self.max_points > MAX_AWARD_POINTS
        {
            return Err(TrainingError::InvalidPolicy);
        }
        Ok(())
    }

    fn points(&self, elapsed_ms: u64) -> u32 {
        ((u128::from(elapsed_ms / self.tick_ms) * u128::from(self.points_per_tick))
            .min(u128::from(self.max_points))) as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "kebab-case", deny_unknown_fields)]
enum JobStatus {
    Active,
    Settled { server_ms: u64 },
    Canceled { server_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Job {
    id: u64,
    stat: Stat,
    started_ms: u64,
    deadline_ms: u64,
    policy: TrainingPolicy,
    status: JobStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredLedger {
    schema_version: u8,
    target_chao_id: String,
    next_job_id: u64,
    last_server_ms: u64,
    job: Option<Job>,
}

#[derive(Debug, Clone)]
pub struct TrainingLedger {
    state: StoredLedger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingAward {
    pub target_chao_id: String,
    pub job_id: u64,
    pub stat: Stat,
    pub points: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub job_id: u64,
    pub stat: Stat,
    pub started_server_ms: u64,
    pub deadline_server_ms: u64,
    pub phase: &'static str,
    pub remaining_ms: u64,
    /// Informational preview only; cannot be submitted as a stat award.
    pub projected_points: u32,
}

impl TrainingLedger {
    pub fn new(target_chao_id: impl Into<String>) -> Result<Self, TrainingError> {
        let id = target_chao_id.into();
        if id.is_empty() || id.len() > 128 {
            return Err(TrainingError::InvalidTarget);
        }
        Ok(Self {
            state: StoredLedger {
                schema_version: 1,
                target_chao_id: id,
                next_job_id: 1,
                last_server_ms: 0,
                job: None,
            },
        })
    }

    fn check_clock(&self, server_now_ms: u64) -> Result<(), TrainingError> {
        if server_now_ms < self.state.last_server_ms {
            return Err(TrainingError::ClockMovedBackwards);
        }
        Ok(())
    }

    pub fn start(
        &mut self,
        stat: Stat,
        duration_ms: u64,
        server_now_ms: u64,
        policy: TrainingPolicy,
    ) -> Result<u64, TrainingError> {
        self.check_clock(server_now_ms)?;
        policy.validate()?;
        if self
            .state
            .job
            .as_ref()
            .is_some_and(|job| job.status == JobStatus::Active)
        {
            return Err(TrainingError::ActiveJob);
        }
        if duration_ms < policy.tick_ms
            || duration_ms > policy.max_duration_ms
            || !duration_ms.is_multiple_of(policy.tick_ms)
        {
            return Err(TrainingError::InvalidDuration);
        }
        let deadline_ms = server_now_ms
            .checked_add(duration_ms)
            .ok_or(TrainingError::TimestampOverflow)?;
        let next = self
            .state
            .next_job_id
            .checked_add(1)
            .ok_or(TrainingError::IdExhausted)?;
        let id = self.state.next_job_id;
        self.state.job = Some(Job {
            id,
            stat,
            started_ms: server_now_ms,
            deadline_ms,
            policy,
            status: JobStatus::Active,
        });
        self.state.next_job_id = next;
        self.state.last_server_ms = server_now_ms;
        Ok(id)
    }

    /// Exactly one award is produced by this ledger transition. The caller
    /// must atomically persist this state and apply the award to the one
    /// shared desktop/phone Chao record before replying to either device.
    pub fn settle(
        &mut self,
        job_id: u64,
        server_now_ms: u64,
    ) -> Result<TrainingAward, TrainingError> {
        self.check_clock(server_now_ms)?;
        let job = self.state.job.as_mut().ok_or(TrainingError::MissingJob)?;
        if job.id != job_id {
            return Err(TrainingError::WrongJob);
        }
        if job.status != JobStatus::Active {
            return Err(TrainingError::JobClosed);
        }
        if server_now_ms < job.deadline_ms {
            return Err(TrainingError::NotReady {
                remaining_ms: job.deadline_ms - server_now_ms,
            });
        }
        let award = TrainingAward {
            target_chao_id: self.state.target_chao_id.clone(),
            job_id,
            stat: job.stat,
            points: job.policy.points(job.deadline_ms - job.started_ms),
        };
        job.status = JobStatus::Settled {
            server_ms: server_now_ms,
        };
        self.state.last_server_ms = server_now_ms;
        Ok(award)
    }

    /// This adaptation cancels an unfinished job without an award. A finished
    /// active job must be settled, so cancel cannot discard completed work.
    pub fn cancel(&mut self, job_id: u64, server_now_ms: u64) -> Result<(), TrainingError> {
        self.check_clock(server_now_ms)?;
        let job = self.state.job.as_mut().ok_or(TrainingError::MissingJob)?;
        if job.id != job_id {
            return Err(TrainingError::WrongJob);
        }
        if job.status != JobStatus::Active {
            return Err(TrainingError::JobClosed);
        }
        if server_now_ms >= job.deadline_ms {
            return Err(TrainingError::ReadyToSettle);
        }
        job.status = JobStatus::Canceled {
            server_ms: server_now_ms,
        };
        self.state.last_server_ms = server_now_ms;
        Ok(())
    }

    pub fn view(&self, server_now_ms: u64) -> Result<Option<JobView>, TrainingError> {
        self.check_clock(server_now_ms)?;
        Ok(self.state.job.as_ref().map(|job| {
            let (phase, until) = match job.status {
                JobStatus::Active => (
                    if server_now_ms >= job.deadline_ms {
                        "ready"
                    } else {
                        "training"
                    },
                    server_now_ms,
                ),
                JobStatus::Settled { server_ms } => ("settled", server_ms),
                JobStatus::Canceled { server_ms } => ("canceled", server_ms),
            };
            let elapsed = until
                .saturating_sub(job.started_ms)
                .min(job.deadline_ms - job.started_ms);
            JobView {
                job_id: job.id,
                stat: job.stat,
                started_server_ms: job.started_ms,
                deadline_server_ms: job.deadline_ms,
                phase,
                remaining_ms: if job.status == JobStatus::Active {
                    job.deadline_ms.saturating_sub(server_now_ms)
                } else {
                    0
                },
                projected_points: if matches!(job.status, JobStatus::Canceled { .. }) {
                    0
                } else {
                    job.policy.points(elapsed)
                },
            }
        }))
    }

    pub fn to_stored_json(&self) -> Result<String, TrainingError> {
        serde_json::to_string(&self.state)
            .map_err(|error| TrainingError::Serialization(error.to_string()))
    }

    /// Trusted database state only. Do not expose this as a client import or
    /// save-upload endpoint; a forged timestamp is not authenticated by JSON.
    pub fn from_trusted_json(json: &str, expected_chao_id: &str) -> Result<Self, TrainingError> {
        let state: StoredLedger = serde_json::from_str(json)
            .map_err(|error| TrainingError::Serialization(error.to_string()))?;
        if state.target_chao_id != expected_chao_id {
            return Err(TrainingError::StoredTargetMismatch);
        }
        if state.schema_version != 1
            || state.target_chao_id.is_empty()
            || state.target_chao_id.len() > 128
            || state.next_job_id == 0
        {
            return Err(TrainingError::InvalidStoredState);
        }
        if let Some(job) = &state.job {
            job.policy
                .validate()
                .map_err(|_| TrainingError::InvalidStoredState)?;
            let duration = job
                .deadline_ms
                .checked_sub(job.started_ms)
                .ok_or(TrainingError::InvalidStoredState)?;
            if job.id == 0
                || job.id >= state.next_job_id
                || job.started_ms > state.last_server_ms
                || duration < job.policy.tick_ms
                || duration > job.policy.max_duration_ms
                || !duration.is_multiple_of(job.policy.tick_ms)
            {
                return Err(TrainingError::InvalidStoredState);
            }
            match job.status {
                JobStatus::Settled { server_ms }
                    if server_ms < job.deadline_ms || server_ms > state.last_server_ms =>
                {
                    return Err(TrainingError::InvalidStoredState);
                }
                JobStatus::Canceled { server_ms }
                    if server_ms < job.started_ms
                        || server_ms >= job.deadline_ms
                        || server_ms > state.last_server_ms =>
                {
                    return Err(TrainingError::InvalidStoredState);
                }
                _ => {}
            }
        }
        Ok(Self { state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> TrainingLedger {
        TrainingLedger::new("server-owned-chao-1").unwrap()
    }

    #[test]
    fn elapsed_time_is_server_measured_and_grant_is_once_only() {
        let mut ledger = ledger();
        let id = ledger
            .start(Stat::Run, 120_000, 1_000, TrainingPolicy::default())
            .unwrap();
        assert_eq!(
            ledger.settle(id, 120_999),
            Err(TrainingError::NotReady { remaining_ms: 1 })
        );
        let result = ledger.settle(id, 121_000).unwrap();
        assert_eq!(result.points, 2);
        assert_eq!(result.stat, Stat::Run);
        assert_eq!(ledger.settle(id, 121_001), Err(TrainingError::JobClosed));
    }

    #[test]
    fn long_absence_cannot_exceed_the_job_or_award_cap() {
        let mut ledger = ledger();
        let id = ledger
            .start(Stat::Power, MAX_DURATION_MS, 1, TrainingPolicy::default())
            .unwrap();
        assert_eq!(ledger.settle(id, u64::MAX).unwrap().points, 480);
    }

    #[test]
    fn rejects_invalid_durations_policy_clock_and_duplicate_jobs() {
        let mut ledger = ledger();
        for duration in [0, 59_999, 60_001, MAX_DURATION_MS + 60_000] {
            assert_eq!(
                ledger.start(Stat::Fly, duration, 1_000, TrainingPolicy::default()),
                Err(TrainingError::InvalidDuration)
            );
        }
        assert!(TrainingPolicy::from_server_config(0, 1, MAX_DURATION_MS, 1).is_err());
        assert!(TrainingPolicy::from_server_config(1, 1, MAX_DURATION_MS + 1, 1).is_err());
        let id = ledger
            .start(Stat::Fly, 60_000, 1_000, TrainingPolicy::default())
            .unwrap();
        assert_eq!(
            ledger.start(Stat::Swim, 60_000, 1_000, TrainingPolicy::default()),
            Err(TrainingError::ActiveJob)
        );
        assert_eq!(
            ledger.settle(id, 999),
            Err(TrainingError::ClockMovedBackwards)
        );
        assert_eq!(ledger.cancel(id + 1, 1_001), Err(TrainingError::WrongJob));
    }

    #[test]
    fn cancel_closes_once_and_cannot_cancel_a_newer_job() {
        let mut ledger = ledger();
        let id = ledger
            .start(Stat::Stamina, 120_000, 1_000, TrainingPolicy::default())
            .unwrap();
        ledger.cancel(id, 30_000).unwrap();
        assert_eq!(ledger.view(30_000).unwrap().unwrap().projected_points, 0);
        assert_eq!(ledger.cancel(id, 30_001), Err(TrainingError::JobClosed));
        assert_eq!(ledger.settle(id, 500_000), Err(TrainingError::JobClosed));
        let next = ledger
            .start(Stat::Swim, 60_000, 30_001, TrainingPolicy::default())
            .unwrap();
        assert!(next > id);
        assert_eq!(ledger.cancel(id, 30_002), Err(TrainingError::WrongJob));
        assert_eq!(ledger.settle(next, 90_001).unwrap().points, 1);
    }

    #[test]
    fn completed_work_must_be_settled_and_clock_cannot_overflow() {
        let mut ledger = ledger();
        assert_eq!(
            ledger.start(Stat::Power, 60_000, u64::MAX, TrainingPolicy::default()),
            Err(TrainingError::TimestampOverflow)
        );
        let id = ledger
            .start(Stat::Swim, 60_000, 0, TrainingPolicy::default())
            .unwrap();
        assert_eq!(ledger.cancel(id, 60_000), Err(TrainingError::ReadyToSettle));
        assert_eq!(ledger.settle(id, 60_000).unwrap().points, 1);
    }

    #[test]
    fn persisted_active_and_settled_states_survive_restart_without_reaward() {
        let mut initial = ledger();
        let id = initial
            .start(Stat::Run, 60_000, 500, TrainingPolicy::default())
            .unwrap();
        let saved = initial.to_stored_json().unwrap();
        let mut resumed = TrainingLedger::from_trusted_json(&saved, "server-owned-chao-1").unwrap();
        assert_eq!(resumed.settle(id, 60_500).unwrap().points, 1);
        let mut reloaded = TrainingLedger::from_trusted_json(
            &resumed.to_stored_json().unwrap(),
            "server-owned-chao-1",
        )
        .unwrap();
        assert_eq!(reloaded.settle(id, 70_000), Err(TrainingError::JobClosed));
    }

    #[test]
    fn trusted_restore_still_validates_target_schema_and_bounded_values() {
        let mut ledger = ledger();
        ledger
            .start(Stat::Run, 60_000, 500, TrainingPolicy::default())
            .unwrap();
        let saved = ledger.to_stored_json().unwrap();
        assert!(matches!(
            TrainingLedger::from_trusted_json(&saved, "other-chao"),
            Err(TrainingError::StoredTargetMismatch)
        ));
        let mut value: serde_json::Value = serde_json::from_str(&saved).unwrap();
        value["job"]["policy"]["tickMs"] = 0.into();
        assert!(
            TrainingLedger::from_trusted_json(&value.to_string(), "server-owned-chao-1").is_err()
        );
        value["job"]["policy"]["tickMs"] = 60_000.into();
        value["clientSuppliedStatGains"] = 999_999.into();
        assert!(
            TrainingLedger::from_trusted_json(&value.to_string(), "server-owned-chao-1").is_err()
        );
    }

    #[test]
    fn server_policy_is_frozen_and_award_cap_is_enforced() {
        let mut ledger = ledger();
        let policy = TrainingPolicy::from_server_config(60_000, 10, 600_000, 12).unwrap();
        let id = ledger.start(Stat::Fly, 600_000, 0, policy).unwrap();
        assert_eq!(ledger.view(120_000).unwrap().unwrap().projected_points, 12);
        assert_eq!(ledger.settle(id, 600_000).unwrap().points, 12);
    }
}
