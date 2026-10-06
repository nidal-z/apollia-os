from enum import Enum


class JournalNoticeType1Notice(str, Enum):
    DAILY_EXPORT_FAILED = "daily_export_failed"

    def __str__(self) -> str:
        return str(self.value)
