from enum import Enum


class EngineVerdict(str, Enum):
    COMPATIBLE = "compatible"
    INCOMPATIBLE = "incompatible"
    RECOMMENDED = "recommended"

    def __str__(self) -> str:
        return str(self.value)
