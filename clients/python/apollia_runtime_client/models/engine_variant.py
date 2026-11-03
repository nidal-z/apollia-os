from enum import Enum


class EngineVariant(str, Enum):
    CPU = "cpu"
    CUDA_12 = "cuda-12"
    CUDA_13 = "cuda-13"
    METAL = "metal"
    ROCM = "rocm"
    VULKAN = "vulkan"

    def __str__(self) -> str:
        return str(self.value)
