//! [`Live`]: a section of the configuration as a consumer holds it.

use tokio::sync::watch;

/// The receiving end of one section channel.
///
/// [`get`](Live::get) reads the value in force at the moment of the call, for
/// a consumer that reads its setting at each use. [`changed`](Live::changed)
/// answers the new value once after each change, for a consumer that rebuilds
/// something from it between two messages. A value given directly (`From<T>`)
/// never changes: it is how a test, or a runtime started without a file, hands
/// a consumer its setting.
#[derive(Debug, Clone)]
pub struct Live<T>(watch::Receiver<T>);

impl<T: Clone> Live<T> {
    /// A value that never changes.
    pub fn fixed(value: T) -> Self {
        let (_sender, receiver) = watch::channel(value);
        Self(receiver)
    }

    pub(super) fn from_receiver(receiver: watch::Receiver<T>) -> Self {
        Self(receiver)
    }

    /// The value in force.
    pub fn get(&self) -> T {
        self.0.borrow().clone()
    }

    /// The new value when it changed since the last call (or since this copy
    /// was made), then `None` until the next change.
    pub fn changed(&mut self) -> Option<T> {
        match self.0.has_changed() {
            Ok(true) => Some(self.0.borrow_and_update().clone()),
            Ok(false) | Err(_) => None,
        }
    }

    /// Waits for the next change and answers the new value; `None` once the
    /// sending end is gone, so a loop over it ends with the runtime.
    pub async fn next_change(&mut self) -> Option<T> {
        self.0.changed().await.ok()?;
        Some(self.0.borrow_and_update().clone())
    }
}

impl<T: Clone> From<T> for Live<T> {
    fn from(value: T) -> Self {
        Self::fixed(value)
    }
}
