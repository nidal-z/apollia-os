from enum import Enum


class EngineRatingReasonType3Code(str, Enum):
    NO_VULKAN_LOADER = "no_vulkan_loader"

    def __str__(self) -> str:
        return str(self.value)
