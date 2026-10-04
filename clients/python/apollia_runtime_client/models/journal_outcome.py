from enum import Enum


class JournalOutcome(str, Enum):
    FAILURE = "failure"
    REFUSED = "refused"
    SUCCESS = "success"

    def __str__(self) -> str:
        return str(self.value)
