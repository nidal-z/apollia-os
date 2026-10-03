from enum import Enum


class NodeRuntimeStateType0State(str, Enum):
    ABSENT = "absent"

    def __str__(self) -> str:
        return str(self.value)
