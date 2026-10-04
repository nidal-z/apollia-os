from enum import Enum


class PendingKind(str, Enum):
    BATCH_ITEM = "batch_item"
    CONSENT = "consent"
    QUESTION = "question"
    TASK_PAUSE = "task_pause"

    def __str__(self) -> str:
        return str(self.value)
