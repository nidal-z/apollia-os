from enum import Enum


class EngineRefusalCode(str, Enum):
    ARCHIVE_INVALID = "archive_invalid"
    DIGEST_MISMATCH = "digest_mismatch"
    MANIFEST_INVALID = "manifest_invalid"
    NETWORK = "network"
    NO_TRUSTED_KEY = "no_trusted_key"
    PACKAGE_NOT_FOUND = "package_not_found"
    ROLLBACK = "rollback"
    SIGNATURE_INVALID = "signature_invalid"
    SIGNATURE_MISSING = "signature_missing"
    SIGNATURE_UNKNOWN_KEY = "signature_unknown_key"
    STORAGE = "storage"

    def __str__(self) -> str:
        return str(self.value)
