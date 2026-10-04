# PDF fixtures

These documents were written for this test suite and carry no third-party
content. `second-page-blank.pdf` has three pages: text with accented letters,
a blank page, and text again. `owner-password-only.pdf` is the same document
encrypted (RC4, 128 bits) with an owner password and an empty user password, so
it opens without a password. `user-password.pdf` is the same document encrypted
with a user password, so it does not. They are read by
`crates/apollia-sources/tests/pdf_reader.rs`.
