from enum import Enum


class EngineInstallationStateType0State(str, Enum):
    IDLE = "idle"

    def __str__(self) -> str:
        return str(self.value)
