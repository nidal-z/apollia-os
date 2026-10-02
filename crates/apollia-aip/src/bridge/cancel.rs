//! Cancellation of a Python dispatch that is already running.
//!
//! The dispatch coroutine runs under `asyncio.run()` on a blocking thread, so
//! dropping the Rust future that waits for it stops nothing: the thread keeps
//! executing the agent. Stopping it means cancelling the asyncio task from the
//! outside, which only the event loop that owns it can do safely, through
//! `loop.call_soon_threadsafe(task.cancel)`.
//!
//! The coroutine is therefore wrapped before it is run. The wrapper publishes
//! its loop and its task into a shared slot, a plain Python dict, and checks a
//! cancel flag in the same slot before it awaits the agent. Every read and
//! write of the slot happens under the GIL, so a request that lands before the
//! wrapper started, while it runs, or after it finished is handled: before, the
//! flag makes the wrapper refuse to start; while, the task is cancelled at its
//! next await point; after, scheduling on a closed loop fails and is ignored.

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyDict;

/// The wrapper coroutine. Kept in Python because it has to run inside the
/// event loop it publishes.
const WRAPPER_CODE: &std::ffi::CStr = c"import asyncio

async def run_cancellable(coro, slot):
    slot['loop'] = asyncio.get_running_loop()
    slot['task'] = asyncio.current_task()
    if slot.get('cancel'):
        coro.close()
        raise asyncio.CancelledError()
    return await coro
";

static WRAPPER: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

/// Wraps `coroutine` so that it can be cancelled through `slot`.
pub(crate) fn wrap<'py>(
    py: Python<'py>,
    coroutine: &Bound<'py, PyAny>,
    slot: &Py<PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    let wrapper = WRAPPER.get_or_try_init(py, || -> PyResult<Py<PyAny>> {
        let module = PyModule::from_code(
            py,
            WRAPPER_CODE,
            c"apollia_bridge_cancel.py",
            c"apollia_bridge_cancel",
        )?;
        Ok(module.getattr("run_cancellable")?.unbind())
    })?;
    wrapper.bind(py).call1((coroutine, slot.bind(py)))
}

/// Asks the running dispatch to stop at its next await point.
///
/// Safe to call at any moment and more than once.
pub(crate) fn request(slot: &Py<PyDict>) {
    Python::with_gil(|py| {
        let slot = slot.bind(py);
        if let Err(error) = slot.set_item("cancel", true) {
            tracing::event!(tracing::Level::WARN, error = %error, "aip.bridge.cancel.flag_failed");
        }
        let (Ok(Some(event_loop)), Ok(Some(task))) = (slot.get_item("loop"), slot.get_item("task"))
        else {
            return;
        };
        let scheduled = task
            .getattr("cancel")
            .and_then(|cancel| event_loop.call_method1("call_soon_threadsafe", (cancel,)));
        if let Err(error) = scheduled {
            // A loop that already closed has nothing left to cancel.
            tracing::event!(tracing::Level::DEBUG, error = %error, "aip.bridge.cancel.loop_closed");
        }
    });
}

/// Requests cancellation when dropped, unless disarmed first.
///
/// Held by the future that waits for the blocking thread, so a caller that
/// abandons the wait (a timeout, an outer `select!`, a dropped task) also stops
/// the Python side instead of leaving it running unobserved.
pub(crate) struct CancelOnDrop {
    slot: Option<Py<PyDict>>,
}

impl CancelOnDrop {
    pub(crate) fn new(slot: Py<PyDict>) -> Self {
        Self { slot: Some(slot) }
    }

    /// The dispatch finished on its own: nothing to cancel any more.
    pub(crate) fn disarm(&mut self) {
        self.slot = None;
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            request(&slot);
        }
    }
}
