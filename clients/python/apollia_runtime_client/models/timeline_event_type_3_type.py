from enum import Enum

class TimelineEventType3Type(str, Enum):
    LLM_CALL = "llm_call"

    def __str__(self) -> str:
        return str(self.value)
