from enum import Enum

class TimelineEventType0Type(str, Enum):
    TASK_TRANSITION = "task_transition"

    def __str__(self) -> str:
        return str(self.value)
