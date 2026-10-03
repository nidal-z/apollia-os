//! The attached arm of the onboarding commands: each one is a call to its
//! route under `/api/v1/onboarding` on the daemon, with the token of this
//! installation, and a refusal is the daemon's own message.

use apollia_runtime::api::routes_onboarding::{
    AdvancePhaseBody, OnboardingDismissed, OnboardingFinalized, OnboardingProfileBody,
    TriggerOnboardingBody,
};
use apollia_runtime::onboarding::state::{OnboardingState, TriggerResult};

use crate::attached::AttachedClient;

/// `GET /api/v1/onboarding`.
pub(crate) async fn state(client: &AttachedClient) -> Result<OnboardingState, String> {
    Ok(client.get("/api/v1/onboarding").await?)
}

/// `POST /api/v1/onboarding/phase`.
pub(crate) async fn advance(
    client: &AttachedClient,
    phase: &str,
) -> Result<OnboardingState, String> {
    let body = AdvancePhaseBody {
        phase: phase.to_owned(),
    };
    Ok(client.post("/api/v1/onboarding/phase", &body).await?)
}

/// `PUT /api/v1/onboarding/profile`.
pub(crate) async fn set_profile(
    client: &AttachedClient,
    profile: &str,
) -> Result<OnboardingState, String> {
    let body = OnboardingProfileBody {
        profile: profile.to_owned(),
    };
    Ok(client.put("/api/v1/onboarding/profile", &body).await?)
}

/// `POST /api/v1/onboarding/sessions`.
pub(crate) async fn trigger(
    client: &AttachedClient,
    topic: Option<String>,
    profile: Option<String>,
) -> Result<TriggerResult, String> {
    let body = TriggerOnboardingBody { topic, profile };
    Ok(client.post("/api/v1/onboarding/sessions", &body).await?)
}

/// `POST /api/v1/onboarding/resume`.
pub(crate) async fn resume(client: &AttachedClient) -> Result<TriggerResult, String> {
    Ok(client.post("/api/v1/onboarding/resume", &()).await?)
}

/// `POST /api/v1/onboarding/dismiss`.
pub(crate) async fn dismiss(client: &AttachedClient) -> Result<(), String> {
    let _: OnboardingDismissed = client.post("/api/v1/onboarding/dismiss", &()).await?;
    Ok(())
}

/// `POST /api/v1/onboarding/finalize`.
pub(crate) async fn finalize(client: &AttachedClient) -> Result<(), String> {
    let _: OnboardingFinalized = client.post("/api/v1/onboarding/finalize", &()).await?;
    Ok(())
}

/// `GET /api/v1/onboarding/finalized`.
pub(crate) async fn is_finalized(client: &AttachedClient) -> Result<bool, String> {
    let answer: OnboardingFinalized = client.get("/api/v1/onboarding/finalized").await?;
    Ok(answer.finalized)
}

#[cfg(test)]
mod tests;
