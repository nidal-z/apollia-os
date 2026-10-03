from enum import Enum


class EngineChannel(str, Enum):
    PREVIEW = "preview"
    STABLE = "stable"

    def __str__(self) -> str:
        return str(self.value)
