# A directory signed by another key

`engines.json` and its signature, made by a throwaway minisign key that was
deleted once it had signed. No build trusts that key, so `apollia-os engine
import` of this directory is refused by the runtime with
`signature_unknown_key`, the case Track 2 asserts. There are no archives:
the refusal comes before any is read.
