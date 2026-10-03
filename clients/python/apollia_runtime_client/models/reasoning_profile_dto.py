from enum import Enum


class ReasoningProfileDto(str, Enum):
    LEVELS = "levels"
    NONE = "none"
    TOGGLE = "toggle"

    def __str__(self) -> str:
        return str(self.value)
