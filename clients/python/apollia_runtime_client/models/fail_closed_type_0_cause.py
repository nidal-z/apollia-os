from enum import Enum


class FailClosedType0Cause(str, Enum):
    STORE_UNREADABLE = "store_unreadable"

    def __str__(self) -> str:
        return str(self.value)
