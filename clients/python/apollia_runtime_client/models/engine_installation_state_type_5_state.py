from enum import Enum


class EngineInstallationStateType5State(str, Enum):
    FAILED = "failed"

    def __str__(self) -> str:
        return str(self.value)
