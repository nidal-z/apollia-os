from enum import Enum


class TranscribeAudioBodySource(str, Enum):
    API = "api"
    FILE = "file"
    HOTKEY = "hotkey"

    def __str__(self) -> str:
        return str(self.value)
