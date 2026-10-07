from enum import Enum


class EngineInstallationStateType1State(str, Enum):
    VERIFYING = "verifying"

    def __str__(self) -> str:
        return str(self.value)
