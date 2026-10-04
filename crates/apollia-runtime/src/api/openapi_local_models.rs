//! The local-model operations of the OpenAPI document: what is resident,
//! loading, estimating, unloading, and the leases and pins that keep a model
//! loaded.
//!
//! Declared apart from [`super::openapi::ApiDoc`] and merged into it by the
//! [`LocalModels`] modifier, so the main document stays a readable size. The
//! merge keys paths and schemas by name, so the served document is the same
//! as if they were listed there.

use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::routes_llm::loaded::list_loaded_models,
        crate::api::routes_llm::loaded::unload_loaded_model,
        crate::api::routes_llm::load::estimate_model_load,
        crate::api::routes_llm::load::load_model,
        crate::api::routes_llm::load::get_last_loaded_model,
        crate::api::routes_llm::load::reload_last_loaded_model,
        crate::api::routes_llm::residency::open_model_lease,
        crate::api::routes_llm::residency::release_model_lease,
        crate::api::routes_llm::residency::list_model_pins,
        crate::api::routes_llm::residency::set_model_pin,
    ),
    components(schemas(
        crate::api::routes_llm::loaded::LoadedModelDto,
        crate::llama_server::ModelKind,
        crate::residency::LoadOrigin,
        crate::local_engine::guardrails::GuardrailRefusedBody,
        crate::local_engine::guardrails::GuardrailLevel,
        crate::api::routes_llm::loaded::LoadedModelsResponse,
        crate::api::routes_llm::loaded::UnloadModelRequest,
        crate::api::routes_llm::loaded::UnloadModelResponse,
        crate::api::routes_llm::load::LoadModelRequest,
        crate::api::routes_llm::load::ReloadLastRequest,
        crate::api::routes_llm::load::FitDto,
        crate::api::routes_llm::load::GuardrailVerdictDto,
        crate::api::routes_llm::load::GuardrailCheckDto,
        crate::api::routes_llm::load::MemoryEstimateDto,
        crate::api::routes_llm::load::LoadEstimateResponse,
        crate::api::routes_llm::load::LoadModelResponse,
        crate::api::routes_llm::load::LastLoadedModelResponse,
        crate::api::routes_llm::residency::OpenLeaseRequest,
        crate::api::routes_llm::residency::LeaseResponse,
        crate::api::routes_llm::residency::ReleaseLeaseRequest,
        crate::api::routes_llm::residency::ReleaseLeaseResponse,
        crate::api::routes_llm::residency::PinsResponse,
        crate::api::routes_llm::residency::ModelPin,
    ))
)]
struct LocalModelsDoc;

/// Merges the local-model operations into the document it modifies.
pub(crate) struct LocalModels;

impl Modify for LocalModels {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.merge(LocalModelsDoc::openapi());
    }
}
