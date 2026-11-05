from enum import Enum


class GpuCardVendor(str, Enum):
    AMD = "amd"
    INTEL = "intel"
    NVIDIA = "nvidia"
    OTHER = "other"

    def __str__(self) -> str:
        return str(self.value)
