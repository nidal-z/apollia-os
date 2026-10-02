from enum import Enum


class AgentOriginDto(str, Enum):
    INTERFACE = "interface"
    PACKAGE = "package"

    def __str__(self) -> str:
        return str(self.value)
