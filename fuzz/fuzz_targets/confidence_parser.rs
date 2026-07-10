#![no_main]

//! Fuzzes the confidence/citation marker parser, the real
//! `apollia_runtime::analyzers::confidence_parser::parse`. Untrusted input is
//! raw LLM message text. The parser hand-scans bytes and returns a value (no
//! Result), so any panic is a reachable crash.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let _ = apollia_runtime::analyzers::confidence_parser::parse(&input);
});
