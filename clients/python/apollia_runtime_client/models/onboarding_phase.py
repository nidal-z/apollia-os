from enum import Enum


class OnboardingPhase(str, Enum):
    ACQUAINTANCE = "acquaintance"
    AI_SETUP = "ai_setup"
    DONE = "done"
    GRADUATION = "graduation"
    GUIDED_TOUR = "guided_tour"
    PROFILE_CHOICE = "profile_choice"
    WELCOME = "welcome"

    def __str__(self) -> str:
        return str(self.value)
