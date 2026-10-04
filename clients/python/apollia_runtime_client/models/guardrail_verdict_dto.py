from enum import Enum


class GuardrailVerdictDto(str, Enum):
    ALLOWED = "allowed"
    REFUSED = "refused"
    WARNED = "warned"

    def __str__(self) -> str:
        return str(self.value)
