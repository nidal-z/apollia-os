from enum import Enum

class BrokenLinkReason(str, Enum):
    HASH_MISMATCH = "hash_mismatch"
    PREV_HASH_MISMATCH = "prev_hash_mismatch"
    SIGNATURE_INVALID = "signature_invalid"
    UNKNOWN_SIGNING_KEY = "unknown_signing_key"

    def __str__(self) -> str:
        return str(self.value)
