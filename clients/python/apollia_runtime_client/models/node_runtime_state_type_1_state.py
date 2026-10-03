from enum import Enum


class NodeRuntimeStateType1State(str, Enum):
    DOWNLOADING = "downloading"

    def __str__(self) -> str:
        return str(self.value)
