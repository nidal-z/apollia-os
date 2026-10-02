from enum import Enum


class SignatureStatus(str, Enum):
    INVALID = "invalid"
    SIGNED = "signed"
    UNSIGNED = "unsigned"

    def __str__(self) -> str:
        return str(self.value)
