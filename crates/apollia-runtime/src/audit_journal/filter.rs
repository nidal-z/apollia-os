//! The filters a reader narrows the journal with: agent, family, nature,
//! result, period, and the connection a tool belongs to.
//!
//! A filter is applied where the journal is read, on the summary of each
//! entry, so a page holds matching entries only and its size means what it
//! says. Within one criterion the values are alternatives; the criteria add up.

use chrono::{DateTime, NaiveDate, Utc};

use crate::audit_journal::entry::JournalEntry;
use crate::audit_journal::summary::{nature_of, JournalNature, JournalOutcome, JournalSummary};
use crate::audit_journal::JournalEntryKind;

/// What a page of the journal is narrowed to. The default keeps everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JournalFilter {
    /// Agents to keep. An entry is kept when its summary names one of them, or
    /// when its run is one of [`Self::agent_runs`].
    pub agents: Vec<String>,
    /// Runs of the tasks of [`Self::agents`], resolved by the caller.
    pub agent_runs: Vec<String>,
    /// Action families to keep, under their wire names.
    pub families: Vec<String>,
    /// Natures to keep.
    pub natures: Vec<JournalNature>,
    /// Results to keep.
    pub outcomes: Vec<JournalOutcome>,
    /// Prefixes of the tools to keep, such as `mcp:notion/` for the tools of
    /// one connection. An entry is kept when the tool its summary names starts
    /// with one of them; an entry that names no tool is not.
    pub tool_prefixes: Vec<String>,
    /// Keep entries written at or after this instant.
    pub since: Option<DateTime<Utc>>,
    /// Keep entries written before this instant.
    pub until: Option<DateTime<Utc>>,
}

/// A filter value the journal cannot read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JournalFilterError {
    /// The nature is none of the known ones.
    #[error("unknown nature '{0}': expected action, access_change, policy or system")]
    Nature(String),
    /// The result is none of the known ones.
    #[error("unknown result '{0}': expected success, failure or refused")]
    Outcome(String),
    /// The bound of the period is neither a day nor an instant.
    #[error("unreadable date '{0}': expected YYYY-MM-DD or an RFC 3339 instant")]
    Date(String),
}

/// The values of a comma-separated criterion, trimmed, without the empty ones.
pub fn split_values(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The bound of a period: an RFC 3339 instant, or a day read as its first
/// instant in UTC.
pub fn parse_instant(raw: &str) -> Result<DateTime<Utc>, JournalFilterError> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(raw) {
        return Ok(instant.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|start| start.and_utc())
        .ok_or_else(|| JournalFilterError::Date(raw.to_owned()))
}

/// The criteria of a request, each one as the request wrote it:
/// comma-separated values, and the bounds of a period as days or instants.
#[derive(Debug, Clone, Copy, Default)]
pub struct JournalCriteria<'a> {
    /// Agent names.
    pub agents: Option<&'a str>,
    /// Action families, under their wire names.
    pub families: Option<&'a str>,
    /// Natures, under their wire names.
    pub natures: Option<&'a str>,
    /// Results, under their wire names.
    pub outcomes: Option<&'a str>,
    /// Prefixes of tool names, such as `mcp:notion/`.
    pub tool_prefixes: Option<&'a str>,
    /// Start of the period, included.
    pub since: Option<&'a str>,
    /// End of the period, excluded.
    pub until: Option<&'a str>,
}

impl JournalFilter {
    /// The filter `criteria` describe. The runs of the agents are left for the
    /// caller to resolve.
    pub fn from_criteria(criteria: JournalCriteria<'_>) -> Result<Self, JournalFilterError> {
        let natures = split_values(criteria.natures)
            .into_iter()
            .map(|name| JournalNature::parse(&name).ok_or(JournalFilterError::Nature(name)))
            .collect::<Result<Vec<_>, _>>()?;
        let outcomes = split_values(criteria.outcomes)
            .into_iter()
            .map(|name| JournalOutcome::parse(&name).ok_or(JournalFilterError::Outcome(name)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            agents: split_values(criteria.agents),
            agent_runs: Vec::new(),
            families: split_values(criteria.families),
            natures,
            outcomes,
            tool_prefixes: split_values(criteria.tool_prefixes),
            since: criteria.since.map(parse_instant).transpose()?,
            until: criteria.until.map(parse_instant).transpose()?,
        })
    }

    /// Whether an entry of `kind` written at `ts` can match, told without
    /// opening its payload. `false` is final; `true` still has to pass
    /// [`Self::matches`].
    pub fn admits(&self, kind: &JournalEntryKind, ts: &str) -> bool {
        if !self.natures.is_empty() && !self.natures.contains(&nature_of(kind)) {
            return false;
        }
        if self.since.is_none() && self.until.is_none() {
            return true;
        }
        // An entry whose date cannot be read is in no period.
        let Ok(at) = DateTime::parse_from_rfc3339(ts) else {
            return false;
        };
        let at = at.with_timezone(&Utc);
        self.since.is_none_or(|since| at >= since) && self.until.is_none_or(|until| at < until)
    }

    /// Whether `entry`, summarised as `summary`, passes every criterion.
    pub fn matches(&self, entry: &JournalEntry, summary: &JournalSummary) -> bool {
        if !self.admits(&entry.kind, &entry.ts) {
            return false;
        }
        if !self.agents.is_empty() {
            let named = summary
                .agent
                .as_ref()
                .is_some_and(|agent| self.agents.contains(agent));
            if !named && !self.agent_runs.contains(&entry.run_id) {
                return false;
            }
        }
        if !self.families.is_empty()
            && !summary
                .family
                .as_ref()
                .is_some_and(|family| self.families.contains(family))
        {
            return false;
        }
        if !self.outcomes.is_empty()
            && !summary
                .outcome
                .is_some_and(|outcome| self.outcomes.contains(&outcome))
        {
            return false;
        }
        if !self.tool_prefixes.is_empty()
            && !summary.tool.as_ref().is_some_and(|tool| {
                self.tool_prefixes
                    .iter()
                    .any(|prefix| tool.starts_with(prefix.as_str()))
            })
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests;
