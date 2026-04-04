use std::fmt;

use serde::{Deserialize, Serialize};

/// Handle en écriture sur l'EventBus — clonable, partageable entre acteurs.
///
/// Alias public défini dans `apollia-core` pour permettre à `apollia-llm`
/// (et toute autre crate sans dépendance sur `apollia-runtime`) d'émettre
/// des événements sur le bus sans créer de dépendance circulaire.
///
/// La publication est non-bloquante ; si le buffer est plein, l'envoi
/// retourne une erreur silencieusement ignorée (fire-and-forget).
pub type EventBusSender = tokio::sync::broadcast::Sender<RuntimeEvent>;

/// Identifiant unique d'un agent dans le runtime (UUID v4 ou nom slug).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    /// Create a new AgentId with a random UUID v4.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for AgentId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for AgentId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Identifiant unique d'une tâche dans le runtime (UUID v4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(String);

impl TaskId {
    /// Create a new TaskId with a random UUID v4.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s.to_string())
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for TaskId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for TaskId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for TaskId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Catalogue complet des événements du runtime Apollia OS.
///
/// Défini dans `apollia-core` pour éviter les dépendances circulaires :
/// tous les acteurs (`apollia-runtime`, `apollia-oria`, etc.) importent
/// ce type sans créer de cycle.
///
/// Transporté via `tokio::sync::broadcast` par l'`EventBus`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    /// Un agent a été enregistré dans le Registry (état: Initializing).
    AgentRegistered(AgentId),
    /// Un agent a terminé son initialisation et est opérationnel (état: Active).
    AgentReady(AgentId),
    /// Un agent est passé en état dégradé.
    AgentDegraded { agent_id: AgentId, reason: String },
    /// Un agent est en cours d'arrêt (état: Stopping, drain des tâches).
    AgentStopping(AgentId),
    /// Un agent s'est arrêté proprement.
    AgentStopped(AgentId),
    /// Une tâche a démarré sur un agent.
    TaskStarted { agent_id: AgentId, task_id: TaskId },
    /// Une tâche s'est terminée (succès ou échec).
    TaskCompleted {
        agent_id: AgentId,
        task_id: TaskId,
        success: bool,
        /// Text output produced by the agent on success; `None` on failure or when the
        /// backend does not carry output (legacy callers set this to `None`).
        #[serde(default)]
        output: Option<String>,
    },
    /// Une tâche a été annulée.
    TaskCanceled { task_id: TaskId },
    /// Un step a été exécuté dans une tâche.
    StepExecuted {
        task_id: TaskId,
        step: u32,
        tool: Option<String>,
    },
    /// Le circuit breaker d'un outil s'est ouvert.
    ToolCircuitBroken { tool_name: String },
    /// Le circuit breaker d'un outil s'est refermé après recovery.
    ToolCircuitRestored { tool_name: String },
    /// Tous les composants sont prêts — runtime opérationnel.
    AllReady,
    /// Arrêt demandé (SIGTERM ou commande CLI).
    ShutdownRequested,
    /// Erreur fatale non récupérable.
    FatalError(String),

    /// Le chargement d'un agent installé a échoué au boot.
    ///
    /// Émis par le Supervisor lors de l'auto-load des agents installés.
    /// L'agent est ignoré mais le runtime continue (dégradation gracieuse).
    AgentLoadFailed {
        /// Nom de l'agent dont le chargement a échoué.
        name: String,
        /// Message d'erreur détaillant la cause de l'échec.
        error: String,
    },

    /// Un agent a été installé de façon permanente.
    AgentInstalled {
        /// Nom unique de l'agent installé.
        name: String,
        /// Version semver de l'agent.
        version: String,
    },
    /// Un agent installé a été supprimé.
    AgentUninstalled {
        /// Nom de l'agent désinstallé.
        name: String,
    },
    /// Un agent installé a été activé pour l'auto-start au boot.
    AgentEnabled {
        /// Nom de l'agent activé.
        name: String,
    },
    /// Un agent installé a été désactivé (ne sera plus chargé au boot).
    AgentDisabled {
        /// Nom de l'agent désactivé.
        name: String,
    },

    /// Un trigger a été déclenché — tâche soumise au runtime.
    TriggerFired {
        /// Identifiant du trigger qui a produit l'événement.
        trigger_id: String,
        /// Nom de l'agent cible.
        agent: String,
        /// Identifiant de la tâche soumise au TaskRouter.
        task_id: TaskId,
    },
    /// Un trigger a été ignoré (OnBusyPolicy::Drop ou agent occupé).
    TriggerSkipped {
        /// Identifiant du trigger.
        trigger_id: String,
        /// Raison du skip.
        reason: String,
    },
    /// Une erreur s'est produite lors du traitement d'un trigger.
    TriggerError {
        /// Identifiant du trigger.
        trigger_id: String,
        /// Message d'erreur.
        error: String,
    },
    /// Un trigger a été activé via la CLI ou l'API.
    TriggerEnabled {
        /// Identifiant du trigger activé.
        trigger_id: String,
    },
    /// Un trigger a été désactivé via la CLI ou l'API.
    TriggerDisabled {
        /// Identifiant du trigger désactivé.
        trigger_id: String,
    },
    /// Le TriggerEngine a rechargé sa configuration (hot reload ou démarrage initial).
    TriggersReloaded {
        /// Nombre de triggers actifs après rechargement.
        count: usize,
    },

    /// Un backend LLM est en cours de chargement (avant `load()` ou initialisation HTTP).
    LlmModelLoading {
        /// Nom logique du backend tel que configuré dans `apollia.toml`.
        backend: String,
        /// Chemin du fichier `.gguf` (backend local) ou URL de l'API (backend cloud).
        model_path: String,
    },
    /// Un backend LLM est prêt — modèle chargé en mémoire ou connexion cloud vérifiée.
    LlmModelReady {
        /// Nom logique du backend.
        backend: String,
        /// Identifiant du modèle : nom de fichier sans extension (.gguf) ou model_id API.
        model_id: String,
    },
    /// Le chargement d'un backend LLM a échoué — backend ignoré, runtime continue.
    LlmModelFailed {
        /// Nom logique du backend.
        backend: String,
        /// Raison de l'échec (message d'erreur).
        reason: String,
    },
    /// Un appel LLM s'est terminé — émis par `complete_with_observability()`.
    LlmCallCompleted {
        /// Nom logique du backend qui a traité la requête.
        backend: String,
        /// Identifiant du modèle utilisé (e.g. `"claude-sonnet-4-20250514"`).
        model: String,
        /// Identifiant de la tâche ayant déclenché l'appel (`None` hors contexte task).
        task_id: Option<String>,
        /// Identifiant du step ORIA ayant déclenché l'appel (`None` en mode direct).
        step_id: Option<String>,
        /// Nombre de tokens dans le prompt.
        prompt_tokens: u32,
        /// Nombre de tokens générés.
        completion_tokens: u32,
        /// Latence totale de l'appel en millisecondes.
        latency_ms: u64,
        /// Coût estimé en USD (backends cloud uniquement ; `None` pour l'inférence locale).
        cost_usd: Option<f64>,
    },

    // ── Plan / Step events ─────────────────────────────────────
    /// Un `ExecutionPlan` a été généré par le Reasoner et persisté en SQLite.
    PlanGenerated {
        /// Identifiant de la tâche ayant déclenché la planification.
        task_id: TaskId,
        /// Nom de l'agent propriétaire du plan.
        agent_name: String,
        /// Identifiant unique du plan (UUID v4).
        plan_id: String,
        /// Nombre de steps dans le plan.
        step_count: usize,
    },

    /// Un step a démarré son exécution — émis par `ActorLoop` avant chaque appel outil ou LLM.
    StepStarted {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Identifiant du step (ex: `"s1"`).
        step_id: String,
        /// Numéro séquentiel du step dans l'exécution (1-based).
        step_num: usize,
        /// Nombre total de steps dans le plan courant.
        total: usize,
        /// Description en langage naturel du step.
        desc: String,
    },

    /// Un step s'est terminé avec succès — émis par `ActorLoop` après chaque appel réussi.
    StepCompleted {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Identifiant du step.
        step_id: String,
        /// Durée d'exécution du step en millisecondes.
        duration_ms: u64,
    },

    /// Un step a échoué — émis par `ActorLoop` après chaque échec.
    StepFailed {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Identifiant du step.
        step_id: String,
        /// Message d'erreur.
        error: String,
        /// `true` si l'erreur peut déclencher une replanification.
        retryable: bool,
    },

    /// Une replanification a été déclenchée après l'échec d'un step retryable.
    PlanReplanning {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Numéro de la tentative de replanification (1-based).
        attempt: u32,
        /// Identifiant du step qui a échoué et déclenché la replanification.
        failed_step: String,
        /// Raison d'échec du step.
        reason: String,
    },

    /// Tous les steps ont été complétés avec succès — plan terminé.
    PlanCompleted {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Nombre de steps complétés.
        step_count: usize,
        /// Durée totale d'exécution du plan en millisecondes.
        duration_ms: u64,
    },

    /// Le plan a échoué de manière irrémédiable.
    PlanFailed {
        /// Identifiant de la tâche parente.
        task_id: TaskId,
        /// Identifiant du plan.
        plan_id: String,
        /// Raison de l'échec.
        reason: String,
    },

    // ── HITL — Human-in-the-Loop events ────────────────────
    /// Une tâche `input_required` a expiré — annulée automatiquement par le `TimeoutWatcher`.
    ///
    /// Émis par `TimeoutWatcher::scan_and_cancel` pour chaque tâche
    /// dont `input_required_at` dépasse `input_required_timeout`.
    /// Suivi immédiatement de [`RuntimeEvent::TaskCanceled`] pour la même tâche.
    TaskApprovalTimeout {
        /// Identifiant de la tâche expirée.
        task_id: TaskId,
        /// Durée du timeout configurée (en secondes).
        after_secs: u64,
    },

    /// Une tâche est suspendue en attente d'une entrée humaine.
    ///
    /// Émis par ORIA après que la suspension est détectée.
    /// - **Mode Direct** : émis par `ORIAEngine::execute_direct()` quand l'agent
    ///   retourne `AIPResult::input_required()`. `step_id` est `None`.
    /// - **Mode Orchestré** : émis par `ActorLoop::suspend_for_approval()` avant
    ///   d'exécuter un step dont l'outil est dans `tools_requiring_approval`.
    ///   `step_id` est `Some(step.step_id)`.
    TaskInputRequired {
        /// Identifiant de la tâche suspendue.
        task_id: TaskId,
        /// Prompt affiché à l'utilisateur pour prendre sa décision.
        prompt: String,
        /// Identifiant du step en attente d'approbation (Mode Orchestré uniquement).
        ///
        /// `None` pour les suspensions en Mode Direct (toute la tâche est suspendue).
        /// `Some(step_id)` pour les suspensions en Mode Orchestré (un step spécifique).
        step_id: Option<String>,
    },

    /// Une tâche a été reprise après une suspension HITL.
    ///
    /// Émis par le `ResumeHandler` après persistence de la
    /// décision humaine dans SQLite et avant la relance ORIA.
    /// souscrit à cet événement pour relancer `run()` sur l'agent.
    TaskResumed {
        /// Identifiant de la tâche reprise.
        task_id: TaskId,
        /// `true` si l'opérateur a approuvé, `false` si rejeté.
        approved: bool,
    },

    // ── Pipeline events ──────────────────────────
    /// Un run de pipeline a démarré — émis par `PipelineExecutor::execute()`.
    PipelineStarted {
        /// Identifiant unique du run (e.g. `"r-0017"`).
        run_id: String,
        /// Identifiant du pipeline déclaré dans `apollia.toml`.
        pipeline_id: String,
        /// Trigger qui a lancé le run; `None` si démarré manuellement.
        trigger_id: Option<String>,
        /// Nombre de steps dans la définition du pipeline.
        step_count: usize,
    },

    /// Un step a été soumis au TaskRouter et est en cours d'exécution.
    PipelineStepStarted {
        /// Identifiant du run parent.
        run_id: String,
        /// Identifiant du step (tel que déclaré dans `[[pipelines.steps]]`).
        step_id: String,
        /// Tâche soumise au TaskRouter pour ce step.
        task_id: String,
        /// Nom de l'agent cible.
        agent: String,
    },

    /// Un step s'est terminé avec succès.
    PipelineStepCompleted {
        /// Identifiant du run parent.
        run_id: String,
        /// Identifiant du step complété.
        step_id: String,
    },

    /// Un step a échoué ; la politique `on_failure` a été appliquée.
    PipelineStepFailed {
        /// Identifiant du run parent.
        run_id: String,
        /// Identifiant du step qui a échoué.
        step_id: String,
        /// Raison de l'échec.
        reason: String,
        /// Politique appliquée : `"skip"`, `"fallback"` ou `"fail"`.
        on_failure: String,
    },

    /// Un step a été sauté (condition=false ou on_failure=skip).
    PipelineStepSkipped {
        /// Identifiant du run parent.
        run_id: String,
        /// Identifiant du step sauté.
        step_id: String,
        /// Raison du skip (e.g. `"condition=false"`, `"on_failure=skip"`).
        reason: String,
    },

    /// Le pipeline est suspendu en attente d'une approbation HITL.
    PipelineSuspended {
        /// Identifiant du run suspendu.
        run_id: String,
        /// Step en attente d'approbation.
        step_id: String,
        /// Tâche en `input_required`.
        task_id: String,
    },

    /// Le pipeline a repris après une approbation HITL.
    PipelineResumed {
        /// Identifiant du run repris.
        run_id: String,
        /// Step qui a été approuvé.
        step_id: String,
    },

    /// Tous les steps ont complété ou été skippés — pipeline terminé avec succès.
    PipelineCompleted {
        /// Identifiant du run.
        run_id: String,
        /// Identifiant du pipeline.
        pipeline_id: String,
        /// Durée totale du run en millisecondes.
        duration_ms: u64,
    },

    /// Le pipeline a échoué suite à un step avec `on_failure=fail`.
    PipelineFailed {
        /// Identifiant du run.
        run_id: String,
        /// Identifiant du pipeline.
        pipeline_id: String,
        /// Step qui a causé l'échec.
        step_id: String,
        /// Raison de l'échec.
        reason: String,
    },

    // ── Chat events ────────────────────────────────
    /// Une session de chat a été créée.
    ChatSessionCreated {
        /// Identifiant unique de la session.
        session_id: String,
        /// Mode de la session (`"libre"` ou `"agent"`).
        mode: String,
        /// Nom de l'agent associé (mode agent uniquement).
        agent_name: Option<String>,
    },
    /// Une session de chat a été fermée.
    ChatSessionClosed {
        /// Identifiant de la session fermée.
        session_id: String,
    },
    /// Un message utilisateur a été envoyé dans une session.
    ChatMessageSent {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant unique du message.
        message_id: String,
    },
    /// Le runtime a commencé à générer une réponse.
    ChatResponseStarted {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message de réponse.
        message_id: String,
    },
    /// Un token de streaming a été produit par le LLM.
    ChatToken {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message de réponse en cours.
        message_id: String,
        /// Token textuel produit.
        token: String,
    },
    /// La réponse complète a été générée.
    ChatResponseCompleted {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message de réponse.
        message_id: String,
        /// Contenu complet de la réponse.
        content: String,
    },
    /// Une erreur s'est produite dans une session de chat.
    ChatError {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message ayant causé l'erreur (si applicable).
        message_id: Option<String>,
        /// Description de l'erreur.
        error: String,
    },
    /// Un appel outil a démarré dans une session de chat.
    ChatToolCallStarted {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message contenant l'appel outil.
        message_id: String,
        /// Nom de l'outil invoqué.
        tool_name: String,
        /// Aperçu tronqué des arguments d'entrée.
        input_preview: String,
    },
    /// Un appel outil s'est terminé dans une session de chat.
    ChatToolCallCompleted {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message contenant l'appel outil.
        message_id: String,
        /// Nom de l'outil invoqué.
        tool_name: String,
        /// `true` si l'exécution a réussi.
        success: bool,
        /// Aperçu tronqué de la sortie (si disponible).
        output_preview: Option<String>,
    },
    /// Une approbation humaine est requise pour un appel outil dans le chat.
    ChatApprovalRequired {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message contenant l'appel outil.
        message_id: String,
        /// Nom de l'outil nécessitant une approbation.
        tool_name: String,
        /// Prompt affiché à l'utilisateur.
        prompt: String,
    },
    /// L'approbation d'un appel outil a été résolue par l'utilisateur.
    ChatApprovalResolved {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message contenant l'appel outil.
        message_id: String,
        /// Nom de l'outil concerné.
        tool_name: String,
        /// Décision prise (`"accept"`, `"refuse"`, `"always_accept"`).
        decision: String,
    },
    /// L'approbation d'un appel outil a expiré (timeout).
    ChatApprovalTimeout {
        /// Identifiant de la session.
        session_id: String,
        /// Identifiant du message contenant l'appel outil.
        message_id: String,
        /// Nom de l'outil concerné.
        tool_name: String,
    },

    // ── Plan Cache events ────────────────────────
    /// Un plan a été récupéré depuis le cache au lieu d'être généré par le Reasoner.
    PlanCacheHit {
        /// Identifiant de la tâche ayant déclenché la recherche dans le cache.
        task_id: TaskId,
        /// Clé SHA-256 du cache qui a produit le hit.
        cache_key: String,
    },

    // ── Agent Messaging events ────────────────────
    /// Un message a été envoyé entre deux agents via l'AgentMailbox.
    AgentMessageSent {
        /// Nom de l'agent expéditeur.
        from: String,
        /// Nom de l'agent destinataire.
        to: String,
    },

    // ── A2A Invocation events ─────────────────────
    /// Une invocation A2A a démarré — émise par `A2AInvoker` avant la soumission de la tâche.
    ///
    /// Émis en fire-and-forget avant l'appel au TaskRouter.
    /// Suivi de [`RuntimeEvent::A2AInvocationCompleted`] après exécution.
    A2AInvocationStarted {
        /// Nom de l'agent initiateur (Director).
        caller: String,
        /// Nom de l'agent cible (Worker).
        target: String,
        /// Identifiant du skill invoqué.
        skill_id: String,
    },
    /// Une invocation A2A s'est terminée — émise après réception du résultat ou d'un échec.
    ///
    /// `status` vaut `"completed"` en cas de succès ou `"failed"` en cas d'erreur.
    A2AInvocationCompleted {
        /// Nom de l'agent initiateur (Director).
        caller: String,
        /// Nom de l'agent cible (Worker).
        target: String,
        /// Identifiant du skill invoqué.
        skill_id: String,
        /// Statut final : `"completed"` ou `"failed"`.
        status: String,
        /// Durée totale de l'invocation en millisecondes.
        duration_ms: u64,
    },

    // ── A2A Guard events ─────────────────────────
    /// Un garde-fou A2A a bloqué une invocation inter-agents.
    ///
    /// Émis par `A2AInvoker::invoke()` dès qu'une protection automatique
    /// (profondeur max, auto-invocation, timeout cumulé de chaîne) empêche
    /// l'invocation de se poursuivre. L'émission précède le retour de l'erreur.
    A2AGuardTriggered {
        /// Catégorie du garde-fou : `"max_depth"`, `"self_invocation"` ou `"chain_timeout"`.
        guard_type: String,
        /// Nom de l'agent initiateur de l'invocation bloquée.
        caller: String,
        /// Identifiant du skill dont l'invocation a été bloquée.
        skill_id: String,
        /// Message explicatif destiné aux logs et à l'observabilité.
        detail: String,
    },

    // ── Onboarding events ────────────────────────
    /// Émis au premier lancement quand la UserMemory est vide.
    ///
    /// Le frontend intercepte cet événement via SSE pour afficher l'écran
    /// d'accueil onboarding. Le runtime continue de fonctionner normalement
    /// — cet événement est purement informatif et ne bloque rien.
    OnboardingRequired,

    /// Émis quand une session d'onboarding est déclenchée (complet ou partiel).
    ///
    /// Le frontend utilise cet événement pour naviguer vers l'écran de chat
    /// onboarding. `mode` vaut `"full"` pour un onboarding complet ou
    /// `"partial"` pour un topic spécifique.
    OnboardingStarted {
        /// Identifiant de la session chat créée pour l'onboarding.
        session_id: String,
        /// `"full"` ou `"partial"`.
        mode: String,
        /// Topic ciblé si mode partial ; `None` en mode full.
        topic: Option<String>,
    },

    /// Émis quand la machine à états onboarding atteint la phase `Done`.
    ///
    /// Le frontend utilise cet événement pour masquer définitivement le
    /// bandeau de reprise et activer toutes les fonctionnalités de l'application.
    OnboardingCompleted {
        /// Profil choisi par l'utilisateur (`"operator"` ou `"builder"`).
        profile: String,
        /// Durée totale de l'onboarding en secondes.
        duration_sec: u64,
        /// Nombre total d'actions complétées pendant le flux.
        actions_count: u32,
    },

    // ── STT events ───────────────────────────────────
    /// L'enregistrement audio STT a démarré (hotkey activée).
    ///
    /// Émis par `SttFlow` quand l'utilisateur active la hotkey d'enregistrement.
    /// Le frontend utilise cet événement pour afficher l'overlay d'enregistrement.
    SttRecordingStarted,

    /// L'enregistrement audio STT s'est arrêté (hotkey relâchée ou silence détecté).
    ///
    /// Émis par `SttFlow` quand l'enregistrement se termine, avant le lancement
    /// de la transcription. `audio_duration_ms` indique la durée de l'audio capturé.
    SttRecordingStopped {
        /// Durée de l'audio enregistré en millisecondes.
        audio_duration_ms: u64,
    },

    /// Le modèle STT a été chargé avec succès — moteur opérationnel.
    ///
    /// Émis par `SttEngine` après chargement du modèle GGML dans `spawn_blocking`.
    /// Le frontend peut utiliser cet événement pour indiquer que le STT est prêt.
    SttModelLoaded {
        /// Nom du backend utilisé (ex: `"whisper-cpp"`).
        backend: String,
        /// Chemin du fichier modèle chargé.
        model_path: String,
        /// Nom court du modèle (dérivé du nom de fichier sans extension).
        model_name: String,
    },

    /// Une transcription STT s'est terminée avec succès.
    ///
    /// Émis par `SttEngine` après persistance dans `SttRepository` et avant
    /// la réponse au caller. Permet au frontend de rafraîchir la liste des
    /// transcriptions et d'afficher un toast de confirmation.
    SttTranscribed {
        /// Texte complet transcrit.
        text: String,
        /// Langue détectée ou utilisée (code ISO 639-1).
        language: Option<String>,
        /// Source de la transcription (`"hotkey"`, `"file"`, `"api"`).
        source: String,
        /// Durée de l'audio source en millisecondes.
        duration_ms: u64,
        /// Temps de traitement en millisecondes.
        processing_time_ms: u64,
    },

    /// Une erreur s'est produite lors d'une transcription STT.
    ///
    /// Émis par `SttEngine` quand `SttBackend::transcribe()` échoue.
    /// Le frontend peut afficher un toast d'erreur ou une notification.
    SttTranscriptionFailed {
        /// Description de l'erreur.
        reason: String,
    },

    // ── Token Budget events ──────────────────────
    /// Mise à jour du budget de session — émis après chaque appel LLM.
    ///
    /// Émis par `LlmRouter::complete_with_observability` après chaque appel backend.
    /// Le desktop widget écoute cet événement pour afficher le coût en temps réel.
    /// L'émission est non-bloquante (broadcast channel).
    TokenBudgetUpdated {
        /// Coût total de la session en USD depuis le dernier reset.
        session_cost_usd: f64,
        /// Tokens en entrée cumulés depuis le dernier reset.
        total_input_tokens: u64,
        /// Tokens en sortie cumulés depuis le dernier reset.
        total_output_tokens: u64,
        /// Tokens lus depuis le cache cumulés depuis le dernier reset.
        total_cache_read_tokens: u64,
        /// Seuil de coût configuré par l'opérateur en USD. `f64::MAX` si non configuré.
        threshold_usd: f64,
        /// `true` si `session_cost_usd > threshold_usd`.
        threshold_exceeded: bool,
    },

    // ── Context Manager events ───────────────────
    /// Émis par `ContextManager` quand l'historique de conversation a été compacté.
    ///
    /// Déclenché dans la boucle ReAct de `BuiltInChatAgent` quand les messages
    /// accumulés dépassent `context_compact_threshold` × la fenêtre du modèle.
    /// Le système prompt original (messages[0]) est toujours préservé.
    ContextCompacted {
        /// Nombre de caractères dans le résumé généré.
        summary_chars: usize,
        /// Nombre de messages originaux remplacés par le résumé.
        original_messages: usize,
    },

    // ── File Path Extraction events ──────────────
    /// Paths de fichiers extraits depuis la sortie d'une commande bash.
    ///
    /// Émis de façon non-bloquante par `FilePathExtractor::extract_detached` après
    /// chaque exécution bash réussie. Permet à ORIA d'invalider les entrées du cache de
    /// plan pour les fichiers affectés (Principe #5 — Un acteur, une responsabilité).
    BashFilePathsExtracted {
        /// Paths extraits depuis la sortie de la commande bash.
        paths: Vec<std::path::PathBuf>,
    },

    // ── Permission events ────────────────────────
    /// Une invocation d'outil nécessite une approbation humaine.
    ///
    /// Émis par `ToolDispatcher::dispatch()` quand `PermissionEngine::decide()` retourne
    /// `PermissionDecision::NeedsApproval`. Le frontend intercepte cet
    /// événement via SSE pour afficher la boîte de dialogue HITL appropriée.
    PermissionRequired {
        /// Nom de l'outil dont l'invocation est suspendue.
        tool_name: String,
        /// Input JSON sérialisé de l'invocation.
        input: serde_json::Value,
        /// Identifiant unique de cette demande d'approbation (UUID v4).
        request_id: String,
    },

    // ── File Timestamp Cache events ──────────────────
    /// Un fichier lu précédemment a été modifié entre deux accès.
    ///
    /// Émis par `FileTimestampCache::record_read()` quand le `mtime` du fichier
    /// sur disque diffère du `mtime` enregistré lors du dernier accès.
    /// ORIA invalide les entrées du cache de plan pour ce fichier.
    FileModifiedSinceRead {
        /// Chemin absolu du fichier modifié.
        path: std::path::PathBuf,
        /// Timestamp `mtime` lors du dernier accès (millisecondes Unix).
        old_mtime_ms: i64,
        /// Timestamp `mtime` actuel (millisecondes Unix).
        new_mtime_ms: i64,
    },

    // ── Binary Feedback / Plan Alternatives events ──
    /// Deux plans alternatifs ont été générés en parallèle par le Reasoner.
    ///
    /// Émis par `ORIAEngine::run_task_with_alternatives()` après que les deux
    /// plans (conservateur et exploratoire) ont été produits via `tokio::join!`.
    /// Le CLI et le Desktop interceptent cet événement pour afficher les deux
    /// plans et demander à l'opérateur lequel exécuter.
    PlanAlternativesGenerated {
        /// Les deux plans alternatifs produits en parallèle.
        alternatives: crate::plan_alternatives::PlanAlternatives,
    },

    /// L'opérateur a choisi un plan parmi les deux alternatives.
    ///
    /// Émis après que l'opérateur a effectué son choix. Suivi de
    /// `PlanChoiceStore::log_plan_choice()` pour la persistance SQLite.
    PlanChosen {
        /// Choix de l'opérateur avec la corrélation `session_id`.
        choice: crate::plan_alternatives::PlanChoice,
    },
}

impl RuntimeEvent {
    /// Retourne `true` si cet événement réarme le timer d'inactivité.
    ///
    /// Les événements significatifs couvrent les transitions de tâche, l'exécution
    /// de steps et d'outils, les réponses LLM, et les demandes d'approbation humaine.
    pub fn is_significant_for_inactivity(&self) -> bool {
        matches!(
            self,
            RuntimeEvent::TaskStarted { .. }
                | RuntimeEvent::StepCompleted { .. }
                | RuntimeEvent::StepExecuted { .. }
                | RuntimeEvent::LlmCallCompleted { .. }
                | RuntimeEvent::PermissionRequired { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac4_all_variants_exist_and_clone() {
        // GIVEN / WHEN — instancier chaque variante et la cloner
        let variants: Vec<RuntimeEvent> = vec![
            RuntimeEvent::AgentRegistered("agent-1".into()),
            RuntimeEvent::AgentReady("agent-1".into()),
            RuntimeEvent::AgentDegraded {
                agent_id: "agent-1".into(),
                reason: "tool missing".into(),
            },
            RuntimeEvent::AgentStopping("agent-1".into()),
            RuntimeEvent::AgentStopped("agent-1".into()),
            RuntimeEvent::TaskStarted {
                agent_id: "agent-1".into(),
                task_id: "task-1".into(),
            },
            RuntimeEvent::TaskCompleted {
                agent_id: "agent-1".into(),
                task_id: "task-1".into(),
                success: true,
                output: None,
            },
            RuntimeEvent::TaskCanceled {
                task_id: "task-1".into(),
            },
            RuntimeEvent::StepExecuted {
                task_id: "task-1".into(),
                step: 1,
                tool: Some("file_io".into()),
            },
            RuntimeEvent::ToolCircuitBroken {
                tool_name: "bash_executor".into(),
            },
            RuntimeEvent::ToolCircuitRestored {
                tool_name: "bash_executor".into(),
            },
            RuntimeEvent::AllReady,
            RuntimeEvent::ShutdownRequested,
            RuntimeEvent::FatalError("out of memory".into()),
            RuntimeEvent::AgentLoadFailed {
                name: "broken-agent".into(),
                error: "module not found".into(),
            },
            RuntimeEvent::AgentInstalled {
                name: "mon-agent".into(),
                version: "0.1.0".into(),
            },
            RuntimeEvent::AgentUninstalled {
                name: "mon-agent".into(),
            },
            RuntimeEvent::AgentEnabled {
                name: "mon-agent".into(),
            },
            RuntimeEvent::AgentDisabled {
                name: "mon-agent".into(),
            },
            RuntimeEvent::LlmModelLoading {
                backend: "local".into(),
                model_path: "/tmp/model.gguf".into(),
            },
            RuntimeEvent::LlmModelReady {
                backend: "local".into(),
                model_id: "llama3.2-q4".into(),
            },
            RuntimeEvent::LlmModelFailed {
                backend: "local".into(),
                reason: "file not found".into(),
            },
            RuntimeEvent::LlmCallCompleted {
                backend: "anthropic".into(),
                model: "claude-sonnet-4-20250514".into(),
                task_id: Some("task-42".into()),
                step_id: None,
                prompt_tokens: 100,
                completion_tokens: 50,
                latency_ms: 250,
                cost_usd: Some(0.001),
            },
            RuntimeEvent::TriggerFired {
                trigger_id: "rapport-hebdo".into(),
                agent: "rapport-agent".into(),
                task_id: "task-1".into(),
            },
            RuntimeEvent::TriggerSkipped {
                trigger_id: "rapport-hebdo".into(),
                reason: "agent busy, on_busy=drop".into(),
            },
            RuntimeEvent::TriggerError {
                trigger_id: "rapport-hebdo".into(),
                error: "agent not found".into(),
            },
            RuntimeEvent::TriggerEnabled {
                trigger_id: "rapport-hebdo".into(),
            },
            RuntimeEvent::TriggerDisabled {
                trigger_id: "rapport-hebdo".into(),
            },
            RuntimeEvent::TriggersReloaded { count: 3 },
            // ── Mode Orchestré ────────────────────────────────
            RuntimeEvent::PlanGenerated {
                task_id: "task-1".into(),
                agent_name: "mon-agent".into(),
                plan_id: "plan-abc".into(),
                step_count: 4,
            },
            RuntimeEvent::StepStarted {
                task_id: "task-1".into(),
                plan_id: "plan-abc".into(),
                step_id: "s1".into(),
                step_num: 1,
                total: 4,
                desc: "Lire le fichier".into(),
            },
            RuntimeEvent::StepCompleted {
                task_id: "task-1".into(),
                plan_id: "plan-abc".into(),
                step_id: "s1".into(),
                duration_ms: 1200,
            },
            RuntimeEvent::StepFailed {
                task_id: "task-1".into(),
                plan_id: "plan-abc".into(),
                step_id: "s2".into(),
                error: "timeout".into(),
                retryable: true,
            },
            RuntimeEvent::PlanReplanning {
                task_id: "task-1".into(),
                plan_id: "plan-abc".into(),
                attempt: 1,
                failed_step: "s2".into(),
                reason: "timeout".into(),
            },
            RuntimeEvent::PlanCompleted {
                task_id: "task-1".into(),
                plan_id: "plan-abc".into(),
                step_count: 4,
                duration_ms: 15900,
            },
            RuntimeEvent::PlanFailed {
                task_id: "task-1".into(),
                plan_id: "plan-abc".into(),
                reason: "MAX_REPLAN_EXCEEDED".into(),
            },
            // ── HITL ──────────────────────────────────────────
            RuntimeEvent::TaskApprovalTimeout {
                task_id: "task-1".into(),
                after_secs: 86400,
            },
            RuntimeEvent::TaskInputRequired {
                task_id: "task-1".into(),
                prompt: "Confirmer l'envoi ?".into(),
                step_id: None,
            },
            RuntimeEvent::TaskResumed {
                task_id: "task-1".into(),
                approved: true,
            },
            // ── Pipeline ──────────────────────────────────────
            RuntimeEvent::PipelineStarted {
                run_id: "r-0001".into(),
                pipeline_id: "traitement-facture".into(),
                trigger_id: None,
                step_count: 3,
            },
            RuntimeEvent::PipelineStepStarted {
                run_id: "r-0001".into(),
                step_id: "ocr".into(),
                task_id: "t-0001".into(),
                agent: "ocr-agent".into(),
            },
            RuntimeEvent::PipelineStepCompleted {
                run_id: "r-0001".into(),
                step_id: "ocr".into(),
            },
            RuntimeEvent::PipelineStepFailed {
                run_id: "r-0001".into(),
                step_id: "validation".into(),
                reason: "timeout".into(),
                on_failure: "fail".into(),
            },
            RuntimeEvent::PipelineStepSkipped {
                run_id: "r-0001".into(),
                step_id: "archivage".into(),
                reason: "on_failure=skip".into(),
            },
            RuntimeEvent::PipelineSuspended {
                run_id: "r-0001".into(),
                step_id: "comptabilite".into(),
                task_id: "t-0051".into(),
            },
            RuntimeEvent::PipelineResumed {
                run_id: "r-0001".into(),
                step_id: "comptabilite".into(),
            },
            RuntimeEvent::PipelineCompleted {
                run_id: "r-0001".into(),
                pipeline_id: "traitement-facture".into(),
                duration_ms: 9400,
            },
            RuntimeEvent::PipelineFailed {
                run_id: "r-0001".into(),
                pipeline_id: "traitement-facture".into(),
                step_id: "validation".into(),
                reason: "timeout".into(),
            },
            // ── Chat ────────────────────────────────
            RuntimeEvent::ChatSessionCreated {
                session_id: "sess-001".into(),
                mode: "libre".into(),
                agent_name: None,
            },
            RuntimeEvent::ChatSessionClosed {
                session_id: "sess-001".into(),
            },
            RuntimeEvent::ChatMessageSent {
                session_id: "sess-001".into(),
                message_id: "msg-001".into(),
            },
            RuntimeEvent::ChatResponseStarted {
                session_id: "sess-001".into(),
                message_id: "msg-002".into(),
            },
            RuntimeEvent::ChatToken {
                session_id: "sess-001".into(),
                message_id: "msg-002".into(),
                token: "Hello".into(),
            },
            RuntimeEvent::ChatResponseCompleted {
                session_id: "sess-001".into(),
                message_id: "msg-002".into(),
                content: "Hello, world!".into(),
            },
            RuntimeEvent::ChatError {
                session_id: "sess-001".into(),
                message_id: Some("msg-003".into()),
                error: "LLM timeout".into(),
            },
            RuntimeEvent::ChatToolCallStarted {
                session_id: "sess-001".into(),
                message_id: "msg-004".into(),
                tool_name: "bash_executor".into(),
                input_preview: "ls -la".into(),
            },
            RuntimeEvent::ChatToolCallCompleted {
                session_id: "sess-001".into(),
                message_id: "msg-004".into(),
                tool_name: "bash_executor".into(),
                success: true,
                output_preview: Some("file.txt".into()),
            },
            RuntimeEvent::ChatApprovalRequired {
                session_id: "sess-001".into(),
                message_id: "msg-005".into(),
                tool_name: "bash_executor".into(),
                prompt: "Allow bash execution?".into(),
            },
            RuntimeEvent::ChatApprovalResolved {
                session_id: "sess-001".into(),
                message_id: "msg-005".into(),
                tool_name: "bash_executor".into(),
                decision: "accept".into(),
            },
            RuntimeEvent::ChatApprovalTimeout {
                session_id: "sess-001".into(),
                message_id: "msg-005".into(),
                tool_name: "bash_executor".into(),
            },
            // ── Plan Cache ────────────────────────
            RuntimeEvent::PlanCacheHit {
                task_id: "task-1".into(),
                cache_key: "abc123def456".into(),
            },
            // ── A2A Invocation ────────────────────────
            RuntimeEvent::A2AInvocationStarted {
                caller: "director".into(),
                target: "excel-worker".into(),
                skill_id: "read-excel".into(),
            },
            RuntimeEvent::A2AInvocationCompleted {
                caller: "director".into(),
                target: "excel-worker".into(),
                skill_id: "read-excel".into(),
                status: "completed".into(),
                duration_ms: 350,
            },
            // ── A2A Guard ────────────────────────────
            RuntimeEvent::A2AGuardTriggered {
                guard_type: "max_depth".into(),
                caller: "director".into(),
                skill_id: "read-excel".into(),
                detail: "depth 3 reaches max_depth 3".into(),
            },
            // ── Onboarding ──────────────────────────
            RuntimeEvent::OnboardingRequired,
            RuntimeEvent::OnboardingStarted {
                session_id: "sess-123".into(),
                mode: "full".into(),
                topic: None,
            },
            RuntimeEvent::OnboardingCompleted {
                profile: "operator".into(),
                duration_sec: 1200,
                actions_count: 18,
            },
            // ── STT ──────────────────────────────────
            RuntimeEvent::SttRecordingStarted,
            RuntimeEvent::SttRecordingStopped {
                audio_duration_ms: 3200,
            },
            RuntimeEvent::SttModelLoaded {
                backend: "whisper-cpp".into(),
                model_path: "/tmp/model.bin".into(),
                model_name: "whisper-large-v3".into(),
            },
            RuntimeEvent::SttTranscribed {
                text: "Bonjour le monde".into(),
                language: Some("fr".into()),
                source: "hotkey".into(),
                duration_ms: 3000,
                processing_time_ms: 800,
            },
            RuntimeEvent::SttTranscriptionFailed {
                reason: "model not loaded".into(),
            },
            // ── Token Budget ─────────────────────────────────
            RuntimeEvent::TokenBudgetUpdated {
                session_cost_usd: 0.0023,
                total_input_tokens: 300,
                total_output_tokens: 150,
                total_cache_read_tokens: 240,
                threshold_usd: 0.50,
                threshold_exceeded: false,
            },
            // ── Context Manager ───────────────────────────────
            RuntimeEvent::ContextCompacted {
                summary_chars: 3800,
                original_messages: 42,
            },
            // ── File Path Extraction ──────────────────────────
            RuntimeEvent::BashFilePathsExtracted {
                paths: vec![
                    std::path::PathBuf::from("src/main.rs"),
                    std::path::PathBuf::from("/tmp/out.txt"),
                ],
            },
            // ── File Timestamp Cache ──────────────────────────
            RuntimeEvent::FileModifiedSinceRead {
                path: std::path::PathBuf::from("/tmp/config.toml"),
                old_mtime_ms: 1_700_000_000_000,
                new_mtime_ms: 1_700_000_060_000,
            },
        ];

        // THEN — toutes les variantes sont clonables et debuggables
        for event in &variants {
            let cloned = event.clone();
            let debug_str = format!("{:?}", cloned);
            assert!(!debug_str.is_empty());
        }
    }

    #[test]
    fn test_runtime_event_debug_format() {
        // GIVEN
        let event = RuntimeEvent::AgentRegistered("agent-42".into());
        // WHEN
        let s = format!("{:?}", event);
        // THEN
        assert!(s.contains("agent-42"));
    }

    // ── sérialisation JSON ─────────────────────────────────────────

    #[test]
    fn test_ac1_serialisation_plan_generated() {
        // GIVEN
        let event = RuntimeEvent::PlanGenerated {
            task_id: "task-001".into(),
            agent_name: "mon-agent".into(),
            plan_id: "plan-abc".into(),
            step_count: 4,
        };
        // WHEN
        let json = serde_json::to_string(&event).expect("sérialisation échoue");
        // THEN
        assert!(json.contains("plan-abc"));
        assert!(json.contains("\"step_count\":4"));
    }

    #[test]
    fn test_ac1_serialisation_step_started() {
        // GIVEN
        let event = RuntimeEvent::StepStarted {
            task_id: "task-001".into(),
            plan_id: "plan-abc".into(),
            step_id: "s1".into(),
            step_num: 1,
            total: 4,
            desc: "Lire le fichier".into(),
        };
        // WHEN
        let json = serde_json::to_string(&event).expect("sérialisation échoue");
        // THEN
        assert!(json.contains("\"step_num\":1"));
        assert!(json.contains("\"total\":4"));
    }

    #[test]
    fn test_ac1_serialisation_step_failed() {
        // GIVEN
        let event = RuntimeEvent::StepFailed {
            task_id: "task-001".into(),
            plan_id: "plan-abc".into(),
            step_id: "s2".into(),
            error: "timeout".into(),
            retryable: true,
        };
        // WHEN
        let json = serde_json::to_string(&event).expect("sérialisation échoue");
        // THEN
        assert!(json.contains("\"retryable\":true"));
    }

    // ── broadcast via EventBus ─────────────────────────────────────

    #[tokio::test]
    async fn test_ac2_broadcast_plan_generated() {
        // GIVEN
        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let event = RuntimeEvent::PlanGenerated {
            task_id: "t-001".into(),
            agent_name: "agent".into(),
            plan_id: "plan-001".into(),
            step_count: 3,
        };
        // WHEN
        tx.send(event).expect("envoi échoue");
        // THEN
        let received = rx.recv().await.expect("réception échoue");
        if let RuntimeEvent::PlanGenerated { step_count, .. } = received {
            assert_eq!(step_count, 3);
        } else {
            panic!("Mauvais event reçu");
        }
    }

    // ── Onboarding ─────────────────────────────────────────────────

    #[test]
    fn test_onboarding_required_event_serialization() {
        // GIVEN
        let event = RuntimeEvent::OnboardingRequired;
        // WHEN
        let json = serde_json::to_string(&event).expect("serialization failed");
        let restored: RuntimeEvent = serde_json::from_str(&json).expect("deserialization failed");
        // THEN
        assert!(json.contains("OnboardingRequired"));
        assert!(matches!(restored, RuntimeEvent::OnboardingRequired));
    }

    // ── round-trip désérialisation ────────────────────────────────

    #[test]
    fn test_ac3_round_trip_step_failed() {
        // GIVEN
        let original = RuntimeEvent::StepFailed {
            task_id: "task-001".into(),
            plan_id: "plan-abc".into(),
            step_id: "s3".into(),
            error: "memory timeout".into(),
            retryable: true,
        };
        // WHEN
        let json = serde_json::to_string(&original).expect("sérialisation échoue");
        let deserialized: RuntimeEvent =
            serde_json::from_str(&json).expect("désérialisation échoue");
        // THEN
        if let RuntimeEvent::StepFailed { retryable, .. } = deserialized {
            assert!(retryable);
        } else {
            panic!("Mauvais variant après désérialisation");
        }
    }
}

// ── Pipeline event tests ─────────────────────────────────────────
#[cfg(test)]
mod pipeline_event_tests {
    use super::*;

    /// sérialisation / désérialisation de `PipelineStarted`.
    #[test]
    fn test_ac2_pipeline_started_roundtrip() {
        // GIVEN
        let event = RuntimeEvent::PipelineStarted {
            run_id: "r-0017".into(),
            pipeline_id: "traitement-facture".into(),
            trigger_id: Some("factures-auto".into()),
            step_count: 6,
        };
        // WHEN
        let json = serde_json::to_string(&event).unwrap();
        let restored: RuntimeEvent = serde_json::from_str(&json).unwrap();
        // THEN
        assert!(matches!(
            restored,
            RuntimeEvent::PipelineStarted { step_count: 6, .. }
        ));
    }

    /// sérialisation / désérialisation de `PipelineCompleted`.
    #[test]
    fn test_pipeline_completed_roundtrip() {
        // GIVEN
        let event = RuntimeEvent::PipelineCompleted {
            run_id: "r-0017".into(),
            pipeline_id: "traitement-facture".into(),
            duration_ms: 9400,
        };
        // WHEN
        let json = serde_json::to_string(&event).unwrap();
        let restored: RuntimeEvent = serde_json::from_str(&json).unwrap();
        // THEN
        assert!(matches!(
            restored,
            RuntimeEvent::PipelineCompleted {
                duration_ms: 9400,
                ..
            }
        ));
    }

    /// sérialisation / désérialisation de `PipelineStepSkipped`.
    #[test]
    fn test_pipeline_step_skipped_roundtrip() {
        // GIVEN
        let event = RuntimeEvent::PipelineStepSkipped {
            run_id: "r-0017".into(),
            step_id: "alerte-fraude".into(),
            reason: "condition=false".into(),
        };
        // WHEN
        let json = serde_json::to_string(&event).unwrap();
        let restored: RuntimeEvent = serde_json::from_str(&json).unwrap();
        // THEN
        assert!(matches!(restored, RuntimeEvent::PipelineStepSkipped { .. }));
    }

    /// tous les 9 variants Pipeline sont constructibles (zéro warning de compilation).
    #[test]
    fn test_all_pipeline_events_compile() {
        // GIVEN / WHEN — construire chaque variant
        let events: Vec<RuntimeEvent> = vec![
            RuntimeEvent::PipelineStarted {
                run_id: "r".into(),
                pipeline_id: "p".into(),
                trigger_id: None,
                step_count: 1,
            },
            RuntimeEvent::PipelineStepStarted {
                run_id: "r".into(),
                step_id: "s".into(),
                task_id: "t".into(),
                agent: "a".into(),
            },
            RuntimeEvent::PipelineStepCompleted {
                run_id: "r".into(),
                step_id: "s".into(),
            },
            RuntimeEvent::PipelineStepFailed {
                run_id: "r".into(),
                step_id: "s".into(),
                reason: "err".into(),
                on_failure: "fail".into(),
            },
            RuntimeEvent::PipelineStepSkipped {
                run_id: "r".into(),
                step_id: "s".into(),
                reason: "condition=false".into(),
            },
            RuntimeEvent::PipelineSuspended {
                run_id: "r".into(),
                step_id: "s".into(),
                task_id: "t".into(),
            },
            RuntimeEvent::PipelineResumed {
                run_id: "r".into(),
                step_id: "s".into(),
            },
            RuntimeEvent::PipelineCompleted {
                run_id: "r".into(),
                pipeline_id: "p".into(),
                duration_ms: 1000,
            },
            RuntimeEvent::PipelineFailed {
                run_id: "r".into(),
                pipeline_id: "p".into(),
                step_id: "s".into(),
                reason: "err".into(),
            },
        ];
        // THEN
        assert_eq!(events.len(), 9);
    }
}
