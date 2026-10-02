from enum import Enum


class PackageNoticeType2Code(str, Enum):
    MISSING_CONNECTION = "missing_connection"

    def __str__(self) -> str:
        return str(self.value)
