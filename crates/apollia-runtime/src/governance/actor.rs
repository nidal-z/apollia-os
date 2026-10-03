//! The governance actor: the one owner of what decisions read.
//!
//! The actor reads the organisation policy held since boot, the fine rules
//! and the standing consents of `governance.db`, and the developer mode of
//! `apollia.toml`, and publishes them as one [`GovernanceState`] on a
//! `watch` channel. A decision reads the last state published without waiting
//! on anyone, so it can sit on the tool-dispatch path; a write (an "always"
//! answered in the interface) goes through the actor's bounded channel, and
//! the state that follows it is published before the write is acknowledged.
//!
//! What another process writes (the command line, the settings screen) is
//! picked up by the actor itself: it looks at the files once a second and
//! reads them again when one of them changed.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use apollia_permissions::{
    ConsentFamily, ConsentScope, FamilyConsentStore, PolicyVerdict, PrefixRuleEngine,
};
use apollia_tools::ActionFamily;
use tokio::sync::{mpsc, oneshot, watch};

use crate::audit_journal::AuditJournalHandle;
use crate::governance::contract::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, InstallSubject,
};
use crate::governance::decision::{decide, GovernanceState, PolicyRefusals};
use crate::governance::record::{GovernanceEvent, GovernanceRecord};

/// Capacity of the actor's command channel.
const COMMAND_CAPACITY: usize = 64;

/// How often the actor looks for a change written by another process.
const REFRESH_PERIOD: Duration = Duration::from_secs(1);

/// Who a standing consent answered in the interface is recorded as given by.
const USER_GRANTOR: &str = "user";

/// The named setting the developer mode is read from.
const DEVELOPER_MODE_KEY: &str = "desktop.developer_mode";

/// Where the actor reads what it decides from.
pub struct GovernanceSources {
    /// `governance.db`; `None` for a runtime without one, whose decisions
    /// read no rule and no consent.
    pub governance_db: Option<PathBuf>,
    /// `apollia.toml`, where the developer mode is read; `None` keeps it off.
    pub config_path: Option<PathBuf>,
    /// The organisation policy read at boot.
    pub policy: Arc<PolicyVerdict>,
    /// The audit journal the records go to; `None` refuses every record.
    pub journal: Option<AuditJournalHandle>,
    /// The connections the policy forbade at boot, as tool identifiers name
    /// them (see [`crate::governance::ForbiddenConnections::tool_connections`]).
    pub forbidden_connections: std::collections::HashSet<String>,
}

/// Errors of a write through the governance actor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GovernanceWriteError {
    /// No standing consent covers this family: executing code, a tool nobody
    /// classified, or a family that needs no consent.
    #[error("no standing consent covers the family {0:?}")]
    NotConsentable(ActionFamily),
    /// The runtime has no governance database to write to.
    #[error("governance has no database to write to")]
    NoDatabase,
    /// The actor has stopped.
    #[error("the governance actor has stopped")]
    ActorGone,
    /// The consent store refused the write.
    #[error("failed to write the consent: {0}")]
    Store(String),
}

enum Command {
    GrantAlways {
        agent_id: String,
        family: ConsentFamily,
        reply: oneshot::Sender<Result<bool, GovernanceWriteError>>,
    },
    Refresh {
        reply: oneshot::Sender<()>,
    },
}

/// Handle on the governance actor, and the implementation of the governance
/// contract the runtime runs on.
#[derive(Clone)]
pub struct GovernanceHandle {
    tx: Option<mpsc::Sender<Command>>,
    state: watch::Receiver<Arc<GovernanceState>>,
    journal: Option<AuditJournalHandle>,
}

impl GovernanceHandle {
    /// Reads `sources` once, then starts the actor that keeps the state
    /// current. Must be called from within a Tokio runtime.
    pub fn spawn(sources: GovernanceSources) -> Self {
        let initial = Arc::new(load_state(&sources));
        let (state_tx, state) = watch::channel(initial);
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let journal = sources.journal.clone();
        let actor = GovernanceActor {
            fingerprint: fingerprint(&sources),
            sources,
            state_tx,
        };
        tokio::spawn(actor.run(rx));
        Self {
            tx: Some(tx),
            state,
            journal,
        }
    }

    /// A handle on a fixed state, with no actor behind it: nothing it holds
    /// changes and every write is refused. For a runtime mounted without a
    /// governance database, and for tests.
    #[must_use]
    pub fn detached(state: GovernanceState, journal: Option<AuditJournalHandle>) -> Self {
        let (_state_tx, state) = watch::channel(Arc::new(state));
        Self {
            tx: None,
            state,
            journal,
        }
    }

    /// The state the next decision reads.
    #[must_use]
    pub fn state(&self) -> Arc<GovernanceState> {
        Arc::clone(&self.state.borrow())
    }

    /// Whether records reach an audit journal.
    #[must_use]
    pub fn journaled(&self) -> bool {
        self.journal.is_some()
    }

    /// Gives `agent_id` a permanent standing consent for `family`, the
    /// "always" of a consent prompt. Returns `true` when the consent is new.
    /// Once it returns, the next decision reads the consent.
    ///
    /// # Errors
    ///
    /// [`GovernanceWriteError::NotConsentable`] for executing code, a tool
    /// nobody classified, or a family that needs no consent;
    /// [`GovernanceWriteError::NoDatabase`], [`GovernanceWriteError::ActorGone`]
    /// or [`GovernanceWriteError::Store`] when the write cannot be made.
    pub async fn grant_always(
        &self,
        agent_id: &str,
        family: ActionFamily,
    ) -> Result<bool, GovernanceWriteError> {
        let consent = family
            .consent_family()
            .filter(|f| !matches!(f, ConsentFamily::ExecuteCode | ConsentFamily::Read))
            .ok_or(GovernanceWriteError::NotConsentable(family))?;
        let tx = self.tx.as_ref().ok_or(GovernanceWriteError::NoDatabase)?;
        let (reply, answer) = oneshot::channel();
        tx.send(Command::GrantAlways {
            agent_id: agent_id.to_owned(),
            family: consent,
            reply,
        })
        .await
        .map_err(|_| GovernanceWriteError::ActorGone)?;
        answer.await.map_err(|_| GovernanceWriteError::ActorGone)?
    }

    /// Reads the sources again now and returns once the state is published.
    /// A detached handle returns at once.
    pub async fn refresh(&self) {
        let Some(tx) = &self.tx else {
            return;
        };
        let (reply, done) = oneshot::channel();
        if tx.send(Command::Refresh { reply }).await.is_ok() {
            let _ = done.await;
        }
    }
}

impl GovernanceContract for GovernanceHandle {
    fn effective_accesses(
        &self,
        _agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess> {
        declared
            .iter()
            .map(|access| EffectiveAccess {
                access_id: access.access_id.clone(),
                permits: access.permits.clone(),
                sensitive: access.declared_sensitive,
            })
            .collect()
    }

    fn decide(&self, call: &AccessCall<'_>) -> AccessDecision {
        let now = chrono::Utc::now().timestamp();
        let decision = decide(&self.state.borrow(), call, now);
        tracing::event!(
            tracing::Level::DEBUG,
            agent_id = %call.agent_id,
            access_id = %call.access_id,
            decision = ?decision,
            "governance.decided"
        );
        decision
    }

    fn signature_required(&self, subject: &InstallSubject<'_>) -> bool {
        let required = self.state.borrow().require_signature;
        if required {
            tracing::event!(
                tracing::Level::DEBUG,
                package = %subject.package_id,
                "governance.signature.required"
            );
        }
        required
    }

    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        let journal = self
            .journal
            .as_ref()
            .ok_or(GovernanceError::JournalUnavailable)?;
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let draft = GovernanceRecord::draft(event, ts)?;
        tracing::event!(
            tracing::Level::DEBUG,
            subject_id = %event.subject_id(),
            kind = %draft.kind.tag(),
            "governance.recorded"
        );
        journal.append(draft);
        Ok(())
    }
}

/// Modification times of the files the state is read from.
type Fingerprint = Vec<Option<SystemTime>>;

struct GovernanceActor {
    sources: GovernanceSources,
    state_tx: watch::Sender<Arc<GovernanceState>>,
    fingerprint: Fingerprint,
}

impl GovernanceActor {
    async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let mut tick = tokio::time::interval(REFRESH_PERIOD);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                command = rx.recv() => match command {
                    Some(Command::GrantAlways { agent_id, family, reply }) => {
                        let written = self.grant(agent_id, family).await;
                        self.publish().await;
                        let _ = reply.send(written);
                    }
                    Some(Command::Refresh { reply }) => {
                        self.publish().await;
                        let _ = reply.send(());
                    }
                    None => break,
                },
                _ = tick.tick() => {
                    if fingerprint(&self.sources) != self.fingerprint {
                        self.publish().await;
                    }
                }
            }
        }
        tracing::event!(tracing::Level::DEBUG, "governance.actor.stopped");
    }

    async fn grant(
        &self,
        agent_id: String,
        family: ConsentFamily,
    ) -> Result<bool, GovernanceWriteError> {
        let db = self
            .sources
            .governance_db
            .clone()
            .ok_or(GovernanceWriteError::NoDatabase)?;
        let written = tokio::task::spawn_blocking(move || {
            let mut store = FamilyConsentStore::new(&db)?;
            store.grant(&agent_id, family, &ConsentScope::Permanent, USER_GRANTOR)
        })
        .await
        .map_err(|e| GovernanceWriteError::Store(e.to_string()))?
        .map_err(|e| GovernanceWriteError::Store(e.to_string()))?;
        tracing::event!(
            tracing::Level::INFO,
            family = family.as_str(),
            new = written,
            "governance.consent.granted"
        );
        Ok(written)
    }

    async fn publish(&mut self) {
        let sources = GovernanceSources {
            governance_db: self.sources.governance_db.clone(),
            config_path: self.sources.config_path.clone(),
            policy: Arc::clone(&self.sources.policy),
            journal: None,
            forbidden_connections: self.sources.forbidden_connections.clone(),
        };
        let loaded = tokio::task::spawn_blocking(move || {
            let fingerprint = fingerprint(&sources);
            (load_state(&sources), fingerprint)
        })
        .await;
        match loaded {
            Ok((state, fingerprint)) => {
                self.fingerprint = fingerprint;
                self.state_tx.send_replace(Arc::new(state));
            }
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "governance.state.reload_failed"
            ),
        }
    }
}

/// The files the state is read from, with the write-ahead log of the
/// database, where a write lands before it reaches the database file.
fn watched_files(sources: &GovernanceSources) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(db) = &sources.governance_db {
        let mut wal = db.clone().into_os_string();
        wal.push("-wal");
        files.push(db.clone());
        files.push(PathBuf::from(wal));
    }
    files.extend(sources.config_path.clone());
    files
}

fn fingerprint(sources: &GovernanceSources) -> Fingerprint {
    watched_files(sources)
        .iter()
        .map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
        .collect()
}

/// Reads everything a decision reads from `sources`. A database or a
/// configuration that cannot be read is reported and read as empty.
pub(crate) fn load_state(sources: &GovernanceSources) -> GovernanceState {
    let (policy, require_signature) = match sources.policy.as_ref() {
        PolicyVerdict::Valid(valid) => (
            PolicyRefusals {
                families: valid.policy.families.deny.iter().copied().collect(),
                denied: valid.policy.connections.deny.iter().cloned().collect(),
                forbidden_connections: sources.forbidden_connections.clone(),
            },
            valid.policy.packages.require_signature,
        ),
        PolicyVerdict::Absent | PolicyVerdict::Invalid(_) => (PolicyRefusals::default(), false),
    };
    let (rules, consents) = match &sources.governance_db {
        Some(db) if db.exists() => read_store(db),
        _ => (Vec::new(), Default::default()),
    };
    GovernanceState {
        policy,
        require_signature,
        rules,
        consents,
        developer_mode: developer_mode(sources),
        fail_closed: false,
    }
}

fn read_store(
    db: &Path,
) -> (
    Vec<apollia_permissions::PrefixRule>,
    std::collections::HashSet<(String, ConsentFamily)>,
) {
    let rules = PrefixRuleEngine::new(db).and_then(|engine| engine.list_rules());
    let consents = FamilyConsentStore::new(db).and_then(|store| store.list_permanent());
    match (rules, consents) {
        (Ok(rules), Ok(consents)) => (rules, consents.into_iter().collect()),
        (rules, consents) => {
            let error = rules
                .err()
                .or(consents.err())
                .map(|e| e.to_string())
                .unwrap_or_default();
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "governance.store.unreadable"
            );
            (Vec::new(), Default::default())
        }
    }
}

fn developer_mode(sources: &GovernanceSources) -> bool {
    let Some(path) = &sources.config_path else {
        return false;
    };
    let imposed = sources.policy.imposed_settings();
    match apollia_core::config::resolve_setting(path, DEVELOPER_MODE_KEY, &imposed) {
        Ok(setting) => setting.value.as_bool().unwrap_or(false),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "governance.developer_mode.unreadable"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests;
