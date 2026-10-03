from enum import Enum


class InstallRefusalType3Code(str, Enum):
    DOWNGRADE = "downgrade"

    def __str__(self) -> str:
        return str(self.value)
