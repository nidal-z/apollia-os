from enum import Enum


class EngineInstallationKind(str, Enum):
    IMPORT = "import"
    INSTALL = "install"
    UPDATE = "update"

    def __str__(self) -> str:
        return str(self.value)
