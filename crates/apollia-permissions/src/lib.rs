//! Apollia OS — moteur de permissions 3 couches.
//!
//! Évalue chaque invocation d'outil avant son exécution selon trois couches ordonnées :
//!
//! 1. **InjectionDetector** (couche 3, priorité absolue) — bloque les patterns shell dangereux.
//! 2. **SafeList** (couche 1) — auto-approuve les commandes explicitement configurées.
//! 3. **PrefixRuleEngine** (couche 2) — évalue les règles préfixe persistées en SQLite.
//!
//! Toutes les décisions sont enregistrées dans `PermissionAuditLog` (SQLite, immuable).
//!
//! ## Utilisation
//!
//! ```rust,ignore
//! use apollia_permissions::{PermissionEngine, PermissionDecision};
//! use apollia_core::config::PermissionsConfig;
//!
//! let engine = PermissionEngine::new(&config, db_path)?;
//! let decision = engine.decide("bash_executor", &input, &manifest)?;
//! match decision {
//!     PermissionDecision::AutoAllowedSafeList => { /* exécuter */ }
//!     PermissionDecision::NeedsApproval => { /* émettre PermissionRequired */ }
//!     PermissionDecision::AutoDeniedInjection { pattern } => { /* retourner PermissionDenied */ }
//!     _ => {}
//! }
//! ```

pub mod audit_log;
pub mod engine;
pub mod error;
pub mod injection_detector;
pub(crate) mod migrations;
pub mod prefix_rule_engine;
pub mod safe_list;

pub use audit_log::{PermissionAuditEntry, PermissionAuditLog};
pub use engine::{PermissionDecision, PermissionEngine};
pub use error::PermissionError;
pub use injection_detector::{InjectionDetector, StructuralInjectionDetector};
pub use prefix_rule_engine::{PrefixRule, PrefixRuleEngine, RuleAction};
pub use safe_list::SafeList;
