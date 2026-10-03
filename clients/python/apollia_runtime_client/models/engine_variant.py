from enum import Enum


class EngineVariant(str, Enum):
    CPU = "cpu"
    CUDA = "cuda"
    METAL = "metal"
    VULKAN = "vulkan"

    def __str__(self) -> str:
        return str(self.value)
