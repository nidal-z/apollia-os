#![no_main]

//! Fuzzes the byte-index string-truncation helpers that cut untrusted text at
//! a computed index. Each is reached through a `#[cfg(fuzzing)] pub` shim over
//! the real production function:
//!   - `apollia_llm` `truncate_kb` (knowledge-base prompt assembly)
//!   - `apollia_connectors` `truncate` (HTTP response / error bodies)
//!   - `apollia_oria` `message_text_preview` (chat message preview)
//! A cut that lands inside a multibyte code point panics; this target drives
//! that regression. The desktop and project-context sites are covered by unit
//! tests instead (they need a Tauri / async context).

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct TruncInput {
    text: String,
    max: usize,
}

fuzz_target!(|input: TruncInput| {
    let text = input.text;
    // Fold `max` into the string so cuts land inside it, not only on no-ops.
    let max = if text.is_empty() {
        0
    } else {
        input.max % (text.len() + 1)
    };

    let _ = apollia_llm::meta::apollia_coach::__fuzz_truncate_kb(&text);
    let _ = apollia_connectors::http::__fuzz_truncate(&text, max);
    let _ = apollia_oria::context_manager::__fuzz_message_text_preview(&text, max);
});
