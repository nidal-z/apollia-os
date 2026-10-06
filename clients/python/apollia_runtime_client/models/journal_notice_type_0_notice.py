from enum import Enum


class JournalNoticeType0Notice(str, Enum):
    UNSIGNED = "unsigned"

    def __str__(self) -> str:
        return str(self.value)
