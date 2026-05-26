//! Handle interne partagé entre `lifecycle` et `proxy`.
//!
//! Évite le couplage circulaire : `proxy` a besoin d'un accès au
//! `RunnerInner` (process + client HTTP) géré par `lifecycle`, mais on ne
//! peut pas rendre `lifecycle::RunnerInner` `pub` sans casser
//! l'encapsulation (le `Child` n'est pas `Clone`).

use super::client::RunnerClient;

/// Handle séparé du `RunnerInner` privé de `lifecycle.rs`. Réexpose juste
/// ce dont le proxy a besoin : le client HTTP cloneable et le port.
pub(super) struct RunnerInnerHandle {
    pub(super) client: RunnerClient,
    #[allow(dead_code)]
    pub(super) port: u16,
}
