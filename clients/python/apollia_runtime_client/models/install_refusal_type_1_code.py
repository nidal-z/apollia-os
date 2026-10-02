from enum import Enum


class InstallRefusalType1Code(str, Enum):
    SIGNATURE_REQUIRED = "signature_required"

    def __str__(self) -> str:
        return str(self.value)
