from enum import Enum


class PendingAnswer(str, Enum):
    ALWAYS = "always"
    ANSWER = "answer"
    ONCE = "once"
    REFUSE = "refuse"

    def __str__(self) -> str:
        return str(self.value)
