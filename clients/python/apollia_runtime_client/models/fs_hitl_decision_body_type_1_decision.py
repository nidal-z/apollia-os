from enum import Enum


class FsHitlDecisionBodyType1Decision(str, Enum):
    DENY = "deny"

    def __str__(self) -> str:
        return str(self.value)
