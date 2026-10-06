from enum import Enum


class LoadOrigin(str, Enum):
    APP = "app"
    SERVER = "server"

    def __str__(self) -> str:
        return str(self.value)
