from enum import Enum


class EngineInstallationStateType4State(str, Enum):
    INSTALLED = "installed"

    def __str__(self) -> str:
        return str(self.value)
