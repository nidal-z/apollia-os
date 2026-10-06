from enum import Enum


class FailClosedType2Cause(str, Enum):
    JOURNAL_UNAVAILABLE = "journal_unavailable"

    def __str__(self) -> str:
        return str(self.value)
