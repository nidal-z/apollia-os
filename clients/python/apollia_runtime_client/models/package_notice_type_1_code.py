from enum import Enum


class PackageNoticeType1Code(str, Enum):
    UNTRUSTED_PUBLISHER = "untrusted_publisher"

    def __str__(self) -> str:
        return str(self.value)
