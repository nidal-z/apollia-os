from enum import Enum


class PromptTemplateSource(str, Enum):
    ENGINE = "engine"
    PRESET = "preset"
    UNAVAILABLE = "unavailable"

    def __str__(self) -> str:
        return str(self.value)
