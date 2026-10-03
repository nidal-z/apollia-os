from enum import Enum


class SourceKindDto(str, Enum):
    CODE_FOLDER = "code_folder"
    FILE = "file"
    FOLDER = "folder"
    MAIL = "mail"
    MEMORY = "memory"
    PROFILE = "profile"

    def __str__(self) -> str:
        return str(self.value)
