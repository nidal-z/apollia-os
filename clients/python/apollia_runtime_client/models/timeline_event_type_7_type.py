from enum import Enum

class TimelineEventType7Type(str, Enum):
    TASK_COMPLETED = "task_completed"

    def __str__(self) -> str:
        return str(self.value)
