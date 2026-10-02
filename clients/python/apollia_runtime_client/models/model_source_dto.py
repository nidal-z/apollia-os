from enum import Enum


class ModelSourceDto(str, Enum):
    LOCAL = "local"
    SERVER = "server"

    def __str__(self) -> str:
        return str(self.value)
