from enum import Enum


class ConnectionKind(str, Enum):
    FOLDER = "folder"
    MCP = "mcp"
    NATIVE = "native"

    def __str__(self) -> str:
        return str(self.value)
