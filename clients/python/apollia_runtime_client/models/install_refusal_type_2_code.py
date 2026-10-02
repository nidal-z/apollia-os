from enum import Enum


class InstallRefusalType2Code(str, Enum):
    DOWNGRADE = "downgrade"

    def __str__(self) -> str:
        return str(self.value)
