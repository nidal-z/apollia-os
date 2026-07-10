#![no_main]

//! Fuzzes the natural-language automation parser, the real
//! `apollia_llm::parse_automation_description` (re-export of
//! `meta::parse_automation::parse_automation`). Untrusted input is operator
//! text. Known crash: byte-index window slice on multibyte UTF-8.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    // Fixed epoch keeps the harness deterministic (no wall-clock in fuzzing).
    let now = chrono::DateTime::from_timestamp(0, 0).expect("epoch is a valid timestamp");
    let known_agents = [String::from("scheduler"), String::from("mailbot")];
    let _ = apollia_llm::parse_automation_description(&input, now, &known_agents);
});
