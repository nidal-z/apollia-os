from enum import Enum


class EngineInstallationStateType2State(str, Enum):
    DOWNLOADING = "downloading"

    def __str__(self) -> str:
        return str(self.value)
