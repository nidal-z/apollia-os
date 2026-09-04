from enum import Enum

class TimelineEventType6Type(str, Enum):
    HITL_RESOLVED = "hitl_resolved"

    def __str__(self) -> str:
        return str(self.value)
