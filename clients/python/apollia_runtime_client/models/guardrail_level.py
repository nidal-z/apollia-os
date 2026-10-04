from enum import Enum


class GuardrailLevel(str, Enum):
    BALANCED = "balanced"
    CEILING = "ceiling"
    DISABLED = "disabled"
    LENIENT = "lenient"
    STRICT = "strict"

    def __str__(self) -> str:
        return str(self.value)
