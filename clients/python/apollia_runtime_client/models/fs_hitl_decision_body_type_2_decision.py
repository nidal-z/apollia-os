from enum import Enum


class FsHitlDecisionBodyType2Decision(str, Enum):
    ALWAYS_ALLOW = "always_allow"

    def __str__(self) -> str:
        return str(self.value)
