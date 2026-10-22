from enum import Enum


class RemovedKind(str, Enum):
    AGENT_ACCESS = "agent_access"
    CONFIGURATION = "configuration"
    INSTALLED_FILES = "installed_files"
    ONEDRIVE_CHOICE = "onedrive_choice"
    SECRET = "secret"
    SOURCE = "source"
    STATE = "state"
    TOKEN = "token"

    def __str__(self) -> str:
        return str(self.value)
