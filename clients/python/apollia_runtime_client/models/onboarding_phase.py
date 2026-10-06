from enum import Enum


class OnboardingPhase(str, Enum):
    ACQUAINTANCE = "acquaintance"
    AI_SETUP = "ai_setup"
    CONNECTION = "connection"
    DONE = "done"
    EMBEDDINGS = "embeddings"
    FIRST_AGENT = "first_agent"
    FIRST_TASK = "first_task"
    GRADUATION = "graduation"
    GUIDED_TOUR = "guided_tour"
    WELCOME = "welcome"

    def __str__(self) -> str:
        return str(self.value)
