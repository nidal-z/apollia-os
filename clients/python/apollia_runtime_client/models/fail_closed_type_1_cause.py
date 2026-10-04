from enum import Enum


class FailClosedType1Cause(str, Enum):
    POLICY_INVALID = "policy_invalid"

    def __str__(self) -> str:
        return str(self.value)
