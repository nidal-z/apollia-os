from enum import Enum


class ServerLogSource(str, Enum):
    ENGINE = "engine"
    REQUEST = "request"

    def __str__(self) -> str:
        return str(self.value)
