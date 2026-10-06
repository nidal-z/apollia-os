from enum import Enum


class ContextOverflowSetting(str, Enum):
    SLIDING_WINDOW = "sliding_window"
    STOP_AT_LIMIT = "stop_at_limit"
    SUMMARIZE_MIDDLE = "summarize_middle"

    def __str__(self) -> str:
        return str(self.value)
