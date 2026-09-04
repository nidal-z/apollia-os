from enum import Enum

class TimelineEventType1Type(str, Enum):
    STEP_STARTED = "step_started"

    def __str__(self) -> str:
        return str(self.value)
