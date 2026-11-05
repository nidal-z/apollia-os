from enum import Enum


class EngineRatingReasonType4Code(str, Enum):
    NO_NVIDIA_DRIVER = "no_nvidia_driver"

    def __str__(self) -> str:
        return str(self.value)
