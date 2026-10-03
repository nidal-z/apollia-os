from enum import Enum


class InstallRefusalType2Code(str, Enum):
    PUBLISHER_NOT_TRUSTED = "publisher_not_trusted"

    def __str__(self) -> str:
        return str(self.value)
