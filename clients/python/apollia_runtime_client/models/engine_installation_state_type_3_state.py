from enum import Enum


class EngineInstallationStateType3State(str, Enum):
    INSTALLING = "installing"

    def __str__(self) -> str:
        return str(self.value)
