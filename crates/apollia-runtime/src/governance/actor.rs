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
//!
//! What cannot be read never becomes a permission: a database that cannot be
//! read, or a policy deposited and refused, puts the state in fail-closed (see
//! [`crate::governance::FailClosed`]), and the actor announces each change of
//! that state on the event bus.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use apollia_permissions::{
    is_code_executor, is_single_simple_command, ConsentFamily, ConsentScope, FamilyConsentStore,
    PermissionScope, PolicyVerdict, PrefixRule, PrefixRuleEngine, RuleAction,
};
use apollia_tools::ActionFamily;
use tokio::sync::{mpsc, oneshot, watch};

use apollia_core::events::{EventBusSender, RuntimeEvent};

use crate::audit_journal::AuditJournalHandle;
use crate::governance::contract::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, InstallSubject,
};
use crate::governance::decision::{decide, GovernanceState, PolicyRefusals};
use crate::governance::fail_closed::FailClosed;
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
    /// The event bus a change of the fail-closed state is announced on;
    /// `None` announces nothing.
    pub events: Option<EventBusSender>,
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
    /// "Always" on executing code writes a fine rule only for a single simple
    /// command of a code executor, in developer mode.
    #[error("no fine rule can allow this command: {0}")]
    NotACommandRule(&'static str),
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
    WriteCommandRule {
        agent_id: String,
        tool_name: String,
        command: String,
        reply: oneshot::Sender<Result<i64, GovernanceWriteError>>,
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
        let initial = load_state(&sources, None);
        announce(None, initial.fail_closed.as_ref(), sources.events.as_ref());
        let (state_tx, state) = watch::channel(Arc::new(initial));
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

    /// Writes the fine rule an "always" on executing code stands for: `allow`
    /// for `agent_id`, on the code executor `tool_name`, with the whole
    /// `command` as its prefix. Never a grant by tool name: the command must
    /// be a single simple command and the developer mode must be on. Returns
    /// the identifier of the rule; once it returns, the next decision reads
    /// it.
    ///
    /// # Errors
    ///
    /// [`GovernanceWriteError::NotACommandRule`] for a tool that does not run
    /// code, a compound command, or the developer mode off;
    /// [`GovernanceWriteError::NoDatabase`], [`GovernanceWriteError::ActorGone`]
    /// or [`GovernanceWriteError::Store`] when the write cannot be made.
    pub async fn allow_command(
        &self,
        agent_id: &str,
        tool_name: &str,
        command: &str,
    ) -> Result<i64, GovernanceWriteError> {
        if !is_code_executor(tool_name) {
            return Err(GovernanceWriteError::NotACommandRule(
                "the tool does not run code",
            ));
        }
        if command.trim().is_empty() || !is_single_simple_command(command) {
            return Err(GovernanceWriteError::NotACommandRule(
                "the command is not a single simple command",
            ));
        }
        if !self.state().developer_mode {
            return Err(GovernanceWriteError::NotACommandRule(
                "the developer mode is off",
            ));
        }
        let tx = self.tx.as_ref().ok_or(GovernanceWriteError::NoDatabase)?;
        let (reply, answer) = oneshot::channel();
        tx.send(Command::WriteCommandRule {
            agent_id: agent_id.to_owned(),
            tool_name: tool_name.to_owned(),
            command: command.to_owned(),
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
                    Some(Command::WriteCommandRule { agent_id, tool_name, command, reply }) => {
                        let written = self.allow(agent_id, tool_name, command).await;
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

    async fn allow(
        &self,
        agent_id: String,
        tool_name: String,
        command: String,
    ) -> Result<i64, GovernanceWriteError> {
        let db = self
            .sources
            .governance_db
            .clone()
            .ok_or(GovernanceWriteError::NoDatabase)?;
        let rule = PrefixRule {
            tool_name,
            arg_prefix: Some(command),
            action: RuleAction::Allow,
            created_at: chrono::Utc::now().timestamp(),
            created_by_agent: None,
            scope: PermissionScope::Agent,
            agent_id: Some(agent_id),
            expires_at: None,
            ..PrefixRule::default()
        };
        let tool = rule.tool_name.clone();
        let id = tokio::task::spawn_blocking(move || {
            let mut engine = PrefixRuleEngine::new(&db)?;
            engine.add_rule(&rule)
        })
        .await
        .map_err(|e| GovernanceWriteError::Store(e.to_string()))?
        .map_err(|e| GovernanceWriteError::Store(e.to_string()))?;
        tracing::event!(
            tracing::Level::INFO,
            tool = %tool,
            rule_id = id,
            "governance.command_rule.written"
        );
        Ok(id)
    }

    async fn publish(&mut self) {
        let sources = GovernanceSources {
            governance_db: self.sources.governance_db.clone(),
            config_path: self.sources.config_path.clone(),
            policy: Arc::clone(&self.sources.policy),
            journal: None,
            events: None,
            forbidden_connections: self.sources.forbidden_connections.clone(),
        };
        let previous = Arc::clone(&self.state_tx.borrow());
        let kept = Arc::clone(&previous);
        let loaded = tokio::task::spawn_blocking(move || {
            let fingerprint = fingerprint(&sources);
            (load_state(&sources, Some(&kept)), fingerprint)
        })
        .await;
        match loaded {
            Ok((state, fingerprint)) => {
                self.fingerprint = fingerprint;
                announce(
                    previous.fail_closed.as_ref(),
                    state.fail_closed.as_ref(),
                    self.sources.events.as_ref(),
                );
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

/// Reads everything a decision reads from `sources`.
///
/// A refused policy fails closed. A database that cannot be read fails closed
/// too, and keeps in force what `previous` read from it (its rules, its
/// consents, its disabled tools); without a previous state, at startup,
/// nothing is kept. A configuration that cannot be read is reported and leaves
/// the developer mode off.
pub(crate) fn load_state(
    sources: &GovernanceSources,
    previous: Option<&GovernanceState>,
) -> GovernanceState {
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
    let store = match &sources.governance_db {
        Some(db) if db.exists() => read_store(db),
        _ => Ok(StoreState::default()),
    };
    let (store, store_failure) = match store {
        Ok(store) => (store, None),
        Err(detail) => {
            let kept = previous.map_or_else(StoreState::default, |p| StoreState {
                rules: p.rules.clone(),
                consents: p.consents.clone(),
                disabled_tools: p.disabled_tools.clone(),
            });
            (kept, Some(FailClosed::StoreUnreadable { detail }))
        }
    };
    GovernanceState {
        policy,
        require_signature,
        rules: store.rules,
        consents: store.consents,
        disabled_tools: store.disabled_tools,
        developer_mode: developer_mode(sources),
        fail_closed: FailClosed::from_policy(&sources.policy).or(store_failure),
    }
}

/// What `governance.db` holds for a decision.
#[derive(Default)]
struct StoreState {
    rules: Vec<apollia_permissions::PrefixRule>,
    consents: std::collections::HashSet<(String, ConsentFamily)>,
    disabled_tools: Vec<String>,
}

/// Reads the fine rules, the standing consents and the disabled tools of
/// `db`, or says why it cannot.
fn read_store(db: &Path) -> Result<StoreState, String> {
    let read = || -> Result<StoreState, String> {
        let rules = PrefixRuleEngine::new(db)
            .and_then(|engine| engine.list_rules())
            .map_err(|e| e.to_string())?;
        let consents = FamilyConsentStore::new(db)
            .and_then(|store| store.list_permanent())
            .map_err(|e| e.to_string())?;
        let disabled_tools = disabled_tools(db)?;
        Ok(StoreState {
            rules,
            consents: consents.into_iter().collect(),
            disabled_tools,
        })
    };
    read().inspect_err(|error| {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "governance.store.unreadable"
        );
    })
}

/// The tools `db` disables. A database no tool was ever switched in has no
/// table for them, and disables none.
fn disabled_tools(db: &Path) -> Result<Vec<String>, String> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let has_table: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tools')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !has_table {
        return Ok(Vec::new());
    }
    Ok(apollia_tools::NativeToolRegistry::new(db)
        .and_then(|registry| registry.list())
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|tool| !tool.enabled)
        .map(|tool| tool.name)
        .collect())
}

/// Announces a change of the fail-closed state, from `before` to `after`, in
/// the log and on `events`. Nothing changes, nothing is announced.
fn announce(
    before: Option<&FailClosed>,
    after: Option<&FailClosed>,
    events: Option<&EventBusSender>,
) {
    if before == after {
        return;
    }
    match after {
        Some(state) => tracing::event!(
            tracing::Level::ERROR,
            cause = state.cause(),
            detail = %state.detail(),
            "governance.fail_closed.entered"
        ),
        None => tracing::event!(tracing::Level::WARN, "governance.fail_closed.left"),
    }
    if let Some(events) = events {
        let _ = events.send(RuntimeEvent::GovernanceFailClosed {
            active: after.is_some(),
            cause: after.map(|state| state.cause().to_owned()),
            detail: after.map(FailClosed::detail),
        });
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

#[cfg(test)]
mod fail_closed_tests;
