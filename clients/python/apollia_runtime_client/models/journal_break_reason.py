from enum import Enum

class JournalBreakReason(str, Enum):
    GLOBAL_HASH_MISMATCH = "global_hash_mismatch"
    GLOBAL_PREV_HASH_MISMATCH = "global_prev_hash_mismatch"
    GLOBAL_SEQ_GAP = "global_seq_gap"
    GLOBAL_SIGNATURE_INVALID = "global_signature_invalid"
    PER_RUN_BROKEN = "per_run_broken"
    UNKNOWN_SIGNING_KEY = "unknown_signing_key"

    def __str__(self) -> str:
        return str(self.value)
