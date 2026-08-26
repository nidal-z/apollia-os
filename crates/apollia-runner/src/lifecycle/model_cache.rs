//! LRU cache for models loaded in VRAM.
//!
//! Skeleton implementation. The real loading and eviction logic will be
//! filled in once the llama-cpp and whisper backends grow VRAM accounting.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// Minimal LRU cache of loaded models.
///
/// For now it only tracks the list of currently loaded `model_id`s. The
/// details (VRAM size, LRU eviction) will be added once the real backend is
/// wired in.
#[derive(Debug, Default)]
pub struct ModelCache {
    loaded: Mutex<HashMap<String, ModelEntry>>,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub model_id: String,
    pub loaded_at: std::time::SystemTime,
    pub memory_used_mb: u32,
}

impl ModelCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lists the currently loaded `model_id`s.
    pub fn loaded_ids(&self) -> Vec<String> {
        let guard = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        guard.keys().cloned().collect()
    }

    /// Total memory used by the loaded models.
    pub fn total_memory_mb(&self) -> u32 {
        let guard = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        guard.values().map(|e| e.memory_used_mb).sum()
    }

    /// Marks a model as loaded.
    pub fn register(&self, entry: ModelEntry) {
        let mut guard = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        guard.insert(entry.model_id.clone(), entry);
    }

    /// Removes a model from the cache.
    pub fn unregister(&self, model_id: &str) -> bool {
        let mut guard = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        guard.remove(model_id).is_some()
    }
}
