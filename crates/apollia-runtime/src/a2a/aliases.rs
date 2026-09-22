//! Names a client can cancel an invocation by, before the runtime has a task id.
//!
//! `POST /api/v1/a2a/invoke` is synchronous: it answers once the worker has
//! finished, and the task id it carries is therefore only readable after the
//! call it would have let the caller cancel. A client that abandons its own
//! request (the operator pressed stop, the import was called off) could do
//! nothing but wait out the timeout while the model kept generating an answer
//! nobody would read.
//!
//! So the client names the invocation itself, in the request body, and the
//! runtime binds that name to the task as soon as the task is submitted, which
//! happens well before the answer. `DELETE /api/v1/tasks/{id}` then accepts
//! either name.
//!
//! The binding lives exactly as long as the call: bound after the submission,
//! released when the invocation returns, whatever it returns. Nothing persists,
//! because a name that outlived its invocation would let a late `DELETE` cancel
//! whichever task inherited it.

use std::collections::HashMap;
use std::sync::Mutex;

/// Client-chosen names bound to the tasks they currently designate.
///
/// Shared between the delegation path, which binds, and the task routes, which
/// resolve. A plain mutex around a map rather than an actor: every operation is
/// a single map access with no await inside, so there is no lock held across a
/// suspension point and no actor whose mailbox this would belong in.
#[derive(Debug, Default)]
pub struct InvocationAliases {
    bound: Mutex<HashMap<String, String>>,
}

impl InvocationAliases {
    /// Bind `client_id` to `task_id` for the duration of one invocation.
    ///
    /// Returns `false` when the name is already taken, in which case nothing is
    /// bound: two live invocations sharing a name would make a `DELETE`
    /// ambiguous, and cancelling the wrong task is worse than refusing the
    /// name.
    pub fn bind(&self, client_id: &str, task_id: &str) -> bool {
        let Ok(mut bound) = self.bound.lock() else {
            return false;
        };
        match bound.entry(client_id.to_owned()) {
            std::collections::hash_map::Entry::Occupied(_) => false,
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(task_id.to_owned());
                true
            }
        }
    }

    /// The task a client name currently designates, if it designates one.
    pub fn resolve(&self, client_id: &str) -> Option<String> {
        self.bound.lock().ok()?.get(client_id).cloned()
    }

    /// Release a client name, whatever its invocation ended as.
    pub fn release(&self, client_id: &str) {
        if let Ok(mut bound) = self.bound.lock() {
            bound.remove(client_id);
        }
    }

    /// Number of names currently bound, for tests and diagnostics.
    pub fn len(&self) -> usize {
        self.bound.lock().map(|b| b.len()).unwrap_or(0)
    }

    /// Whether no name is currently bound.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_name_resolves_to_its_task() {
        // GIVEN an invocation the client named itself
        let aliases = InvocationAliases::default();

        // WHEN the runtime binds it to the task it submitted
        assert!(aliases.bind("import-42", "task-7f3a"));

        // THEN a DELETE on the client's name reaches the right task, which is
        // the whole point: the task id was not readable before the answer
        assert_eq!(aliases.resolve("import-42").as_deref(), Some("task-7f3a"));
    }

    #[test]
    fn an_unknown_name_resolves_to_nothing() {
        // GIVEN an empty registry
        let aliases = InvocationAliases::default();

        // WHEN a name nobody bound is resolved
        // THEN the caller gets nothing, and the route falls back to reading the
        // id as a task id rather than inventing one
        assert!(aliases.resolve("import-42").is_none());
    }

    #[test]
    fn a_name_already_live_is_refused_rather_than_stolen() {
        // GIVEN a name bound to a running invocation
        let aliases = InvocationAliases::default();
        assert!(aliases.bind("import-42", "task-first"));

        // WHEN a second invocation asks for the same name
        let accepted = aliases.bind("import-42", "task-second");

        // THEN it is refused and the first binding stands, so a DELETE can
        // never cancel a task the caller did not mean
        assert!(!accepted);
        assert_eq!(aliases.resolve("import-42").as_deref(), Some("task-first"));
    }

    #[test]
    fn releasing_frees_the_name_for_the_next_invocation() {
        // GIVEN an invocation that has ended and released its name
        let aliases = InvocationAliases::default();
        aliases.bind("import-42", "task-first");
        aliases.release("import-42");

        // WHEN the next invocation reuses the same name
        let accepted = aliases.bind("import-42", "task-second");

        // THEN it is accepted, and nothing from the previous call survives to
        // be cancelled by a late DELETE
        assert!(accepted);
        assert_eq!(aliases.resolve("import-42").as_deref(), Some("task-second"));
        assert_eq!(aliases.len(), 1);
    }
}
