//! Types pour `/handshake`, `/health`, `/info`.

use serde::{Deserialize, Serialize};

/// Backend backing le runner courant. Annoncé au daemon via `/handshake`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cpu,
    Cuda,
    Rocm,
    Vulkan,
    Metal,
}

impl Backend {
    /// Backend activé à la compile selon les features Cargo.
    pub const fn from_features() -> Self {
        #[cfg(feature = "local-cuda")]
        return Self::Cuda;
        #[cfg(all(feature = "local-rocm", not(feature = "local-cuda")))]
        return Self::Rocm;
        #[cfg(all(
            feature = "local-metal",
            not(feature = "local-cuda"),
            not(feature = "local-rocm")
        ))]
        return Self::Metal;
        #[cfg(all(
            feature = "local-vulkan",
            not(feature = "local-cuda"),
            not(feature = "local-rocm"),
            not(feature = "local-metal")
        ))]
        return Self::Vulkan;
        #[cfg(not(any(
            feature = "local-cuda",
            feature = "local-rocm",
            feature = "local-metal",
            feature = "local-vulkan"
        )))]
        return Self::Cpu;
    }
}

/// Info GPU découverte par le runner au boot.
///
/// `None` pour le backend CPU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfoDto {
    pub vendor: String,
    pub model: String,
    pub memory_total_mb: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_capability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_version: Option<String>,
}

/// Payload de réponse pour `GET /handshake`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeData {
    pub protocol_version: String,
    pub runner_version: String,
    pub backend: Backend,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<GpuInfoDto>,
    pub supported_endpoints: Vec<String>,
    pub max_concurrent_requests: u32,
}

/// Payload de réponse pour `GET /health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthData {
    pub uptime_secs: u64,
    pub loaded_models: Vec<String>,
    pub memory_used_mb: u32,
    pub memory_total_mb: u32,
}
