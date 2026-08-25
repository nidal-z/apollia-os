//! LLM inference backends for `apollia-llm`.
//!
//! Local backends (llama.cpp, whisper) are hosted by the `apollia-runner`
//! crate (multi-runner sidecar). This module now holds only the cloud HTTP
//! clients:
//!
//! - `openai`   : OpenAI-compatible HTTP client `[feature = "cloud"]`
//! - `anthropic`: Anthropic HTTP client `[feature = "cloud"]`
//! - `vertex`   : Google Vertex AI client `[feature = "cloud"]`

#[cfg(feature = "cloud")]
pub mod anthropic;

#[cfg(feature = "cloud")]
pub mod openai;

#[cfg(feature = "cloud")]
pub mod vertex;

/// Serialisation and restoration for tests that mutate the process
/// environment. Environment variables are process globals: two parallel
/// tests that touch them race each other, and a test that fails to restore
/// leaks its value into every test that runs after it in the same binary.
#[cfg(all(test, feature = "cloud"))]
pub(crate) mod test_env {
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Runs `f` with `key` set to `value`, serialised on the shared lock,
    /// restoring the previous value before returning.
    pub(crate) fn with_env_var<F: FnOnce()>(key: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        f();
        match previous {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
