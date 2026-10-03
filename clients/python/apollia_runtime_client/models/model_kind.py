from enum import Enum


class ModelKind(str, Enum):
    CHAT = "chat"
    EMBEDDINGS = "embeddings"

    def __str__(self) -> str:
        return str(self.value)
