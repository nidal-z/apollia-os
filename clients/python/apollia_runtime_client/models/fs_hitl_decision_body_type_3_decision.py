from enum import Enum


class FsHitlDecisionBodyType3Decision(str, Enum):
    ALWAYS_ALLOW_SESSION = "always_allow_session"

    def __str__(self) -> str:
        return str(self.value)
