from enum import Enum


class NodeRuntimeStateType2State(str, Enum):
    READY = "ready"

    def __str__(self) -> str:
        return str(self.value)
