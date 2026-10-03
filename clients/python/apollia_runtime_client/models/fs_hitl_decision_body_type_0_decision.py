from enum import Enum


class FsHitlDecisionBodyType0Decision(str, Enum):
    APPROVE = "approve"

    def __str__(self) -> str:
        return str(self.value)
