from enum import Enum


class IssueItem(str, Enum):
    AGENT = "agent"
    CONVERSATIONS = "conversations"
    DOCUMENT = "document"
    FOLDER = "folder"
    INSTRUCTIONS = "instructions"
    MEMORY = "memory"
    PROVIDER = "provider"
    STORE = "store"

    def __str__(self) -> str:
        return str(self.value)
