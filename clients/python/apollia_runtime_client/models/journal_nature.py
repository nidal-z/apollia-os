from enum import Enum


class JournalNature(str, Enum):
    ACCESS_CHANGE = "access_change"
    ACTION = "action"
    POLICY = "policy"
    SYSTEM = "system"

    def __str__(self) -> str:
        return str(self.value)
