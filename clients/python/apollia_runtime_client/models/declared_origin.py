from enum import Enum


class DeclaredOrigin(str, Enum):
    API = "api"
    CLI = "cli"
    EDITOR = "editor"
    SETTINGS = "settings"

    def __str__(self) -> str:
        return str(self.value)
