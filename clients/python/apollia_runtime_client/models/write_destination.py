from enum import Enum


class WriteDestination(str, Enum):
    COPY = "copy"
    ORIGINAL = "original"

    def __str__(self) -> str:
        return str(self.value)
