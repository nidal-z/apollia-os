from enum import Enum


class EngineRatingReasonType2Code(str, Enum):
    GFX_NOT_IN_BUILD = "gfx_not_in_build"

    def __str__(self) -> str:
        return str(self.value)
