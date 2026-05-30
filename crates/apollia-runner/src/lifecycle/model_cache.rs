//! LRU cache for models loaded in VRAM.
//!
//! Skeleton implementation. The real loading and eviction logic will be
//! filled in once the llama-cpp and whisper backends grow VRAM accounting.

use std::collections::HashMap;
use std::sync::Mutex;

/// Cache LRU simpliste des modèles chargés.
///
/// Pour Phase 1, ne fait que tenir une liste des `model_id` actuellement
/// chargés. Les détails (taille VRAM, LRU eviction) seront ajoutés en
/// Phase 2 quand le backend réel sera câblé.
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

    /// Liste les `model_id` actuellement chargés.
    pub fn loaded_ids(&self) -> Vec<String> {
        let guard = self.loaded.lock().expect("model cache lock poisoned");
        guard.keys().cloned().collect()
    }

    /// Mémoire totale utilisée par les modèles chargés.
    pub fn total_memory_mb(&self) -> u32 {
        let guard = self.loaded.lock().expect("model cache lock poisoned");
        guard.values().map(|e| e.memory_used_mb).sum()
    }

    /// Marque un modèle comme chargé (placeholder pour Phase 2).
    pub fn register(&self, entry: ModelEntry) {
        let mut guard = self.loaded.lock().expect("model cache lock poisoned");
        guard.insert(entry.model_id.clone(), entry);
    }

    /// Retire un modèle du cache.
    pub fn unregister(&self, model_id: &str) -> bool {
        let mut guard = self.loaded.lock().expect("model cache lock poisoned");
        guard.remove(model_id).is_some()
    }
}
