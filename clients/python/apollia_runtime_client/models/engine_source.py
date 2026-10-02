from enum import Enum


class EngineSource(str, Enum):
    DOWNLOADED = "downloaded"
    EMBEDDED = "embedded"
    IMPORTED = "imported"

    def __str__(self) -> str:
        return str(self.value)
