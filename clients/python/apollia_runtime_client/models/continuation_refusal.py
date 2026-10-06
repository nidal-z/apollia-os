from enum import Enum


class ContinuationRefusal(str, Enum):
    AGENT = "agent"
    ENGINE = "engine"
    NO_MODEL = "no_model"

    def __str__(self) -> str:
        return str(self.value)
