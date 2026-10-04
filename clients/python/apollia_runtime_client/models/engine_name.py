from enum import Enum


class EngineName(str, Enum):
    LLAMA_SERVER = "llama-server"
    NODEJS = "nodejs"
    VLLM_MLX = "vllm-mlx"

    def __str__(self) -> str:
        return str(self.value)
