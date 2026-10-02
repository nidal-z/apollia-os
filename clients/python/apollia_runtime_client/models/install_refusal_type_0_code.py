from enum import Enum


class InstallRefusalType0Code(str, Enum):
    INVALID_SIGNATURE = "invalid_signature"

    def __str__(self) -> str:
        return str(self.value)
