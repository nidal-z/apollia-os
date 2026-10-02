from enum import Enum


class PackageNoticeType0Code(str, Enum):
    UNSIGNED = "unsigned"

    def __str__(self) -> str:
        return str(self.value)
