from enum import Enum

class TimelineEventType2Type(str, Enum):
    STEP_COMPLETED = "step_completed"

    def __str__(self) -> str:
        return str(self.value)
