from enum import Enum


class SkipReasonDto(str, Enum):
    TOO_LARGE = "too_large"
    UNREADABLE = "unreadable"

    def __str__(self) -> str:
        return str(self.value)
