//! The pauses governance publishes, read by the tools of a turn at each call.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::watch;

use crate::governance::decision::GovernanceState;

/// The connections in pause as the governance actor last published them: the
/// state the next decision reads, read through the same channel.
pub(super) struct PublishedPauses(pub(super) watch::Receiver<Arc<GovernanceState>>);

impl apollia_tools::PauseReader for PublishedPauses {
    fn paused(&self) -> HashSet<String> {
        self.0.borrow().paused.clone()
    }
}
