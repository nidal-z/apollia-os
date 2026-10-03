from enum import Enum


class NodeRuntimeStateType3State(str, Enum):
    FAILED = "failed"

    def __str__(self) -> str:
        return str(self.value)
