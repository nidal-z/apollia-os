from enum import Enum


class MigrationStatus(str, Enum):
    COMPLETE = "complete"
    PARTIAL = "partial"

    def __str__(self) -> str:
        return str(self.value)
