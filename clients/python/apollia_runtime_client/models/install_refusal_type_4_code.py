from enum import Enum


class InstallRefusalType4Code(str, Enum):
    AGENT_TAKEN = "agent_taken"

    def __str__(self) -> str:
        return str(self.value)
