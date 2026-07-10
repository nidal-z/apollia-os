#![no_main]

//! Fuzzes the web article extractor, the real
//! `apollia_tools::tools::web_read::extract_article_text` (reached through a
//! `#[cfg(fuzzing)] pub` shim). Untrusted input is remote HTML bytes fed to the
//! `dom_smoothie` Readability parser. Guards both our lossy-UTF-8 handling and
//! the third-party DOM parser against crashes.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    apollia_tools::tools::web_read::__fuzz_extract_article_text(data, "https://example.invalid/", true);
});
