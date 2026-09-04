from enum import Enum

class TimelineEventType5Type(str, Enum):
    HITL_SUSPENDED = "hitl_suspended"

    def __str__(self) -> str:
        return str(self.value)
