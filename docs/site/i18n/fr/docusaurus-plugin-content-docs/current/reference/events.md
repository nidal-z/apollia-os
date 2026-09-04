---
sidebar_position: 9
title: Événements du runtime
---

# Événements du runtime

Tout ce que fait le runtime et auquel une autre partie du système peut réagir
circule sous forme d'un `RuntimeEvent` sur un unique bus de diffusion
in-process. L'application de bureau, les flux HTTP, le moteur de notifications,
le journal d'audit et le magasin d'observabilité sont tous lecteurs de ce même
bus.

Cette page est le catalogue. Elle est générée depuis la source Rust : elle dit
donc ce que le binaire transporte, et non ce qu'une conception antérieure
prévoyait.

## Ce qu'est un nom de variante

Un nom de variante est un contrat de fil. Il atteint l'application de bureau
dans l'enveloppe `runtime-event` sous le champ `event_type`, et il atteint un
client HTTP par les flux d'événements de tâche et de conversation. En renommer
une casse tout lecteur qui filtrait dessus : un renommage est un changement de
format de fil, pas un remaniement.

## Les catégories, et pourquoi elles comptent plus que les noms

Le pont de bureau ne transmet pas des noms de variantes que l'interface aurait à
trier. Il attache une **catégorie**, et la vue web se branche dessus : une
catégorie correspond à un rafraîchissement, un magasin, un panneau. Une variante
ajoutée à une catégorie existante est lue par ce qui lit déjà cette catégorie ;
une variante dotée d'une catégorie neuve n'est lue par personne tant qu'aucun
auditeur n'existe pour elle.

C'est cette asymétrie qui met la catégorie dans le tableau ci-dessous. Trois
variantes se trouvaient dans des catégories qu'aucun auditeur ne lisait :
l'interface les recevait et n'en faisait rien, ce qu'aucun test et aucun
compilateur ne pouvait voir.

## Le retard

Le bus est un anneau borné. Un abonné qui décroche reçoit un signalement de
retard plutôt que les événements manqués. La règle tient en une ligne et vit en
un seul endroit, `apollia_core::events::ResilientReceiver` : journaliser un
`WARN` nommant l'abonné et le nombre d'événements perdus, se réabonner en queue,
et poursuivre. Ne jamais paniquer sur un retard, et ne jamais perdre des
événements sans le dire.

Les routes d'événements envoyés par le serveur sont la seule exception, nommée
comme telle : elles confient un flux à la couche HTTP et un flux possède son
récepteur, il n'y a donc rien à réabonner. Elles gardent la moitié de la règle
qui reste atteignable, le `WARN`.

## Le catalogue

Le tableau porte les noms de variantes et la forme de leur charge utile. Les
descriptions vivent dans les commentaires de documentation de la source, qui
sont en anglais, et la règle « une langue par fichier » interdit de les servir
ici : la page anglaise les rend.

Avant le tableau, une réserve qu'il ne porte pas. `HookDecisionRecorded` rapporte
la décision d'un hook `PreToolUse`, et `PreToolUse` ne fait pas partie de la
surface prise en charge de `v0.1.0-preview`. Sa décision est appliquée au mieux :
un gestionnaire qui expire, dont la livraison échoue, ou qui répond quelque chose
d'illisible retombe sur `allow`, et l'appel d'outil passe.

Une seconde, sur `PermissionRequired`. Sa description anglaise annonce
l'approbation humaine d'un appel d'outil, première ligne d'un commentaire de
documentation dont le tableau ne peut pas porter la suite. L'unique émetteur de
production est la garde de la boîte aux lettres des agents : il se déclenche
quand un envoi d'agent à agent sous garde est refusé, avec `tool_name` valant
`mailbox:send`. L'envoi n'attend aucune réponse, il lève immédiatement, si bien
que le `request_id` porté par l'événement annonce une demande déjà tranchée.

<!-- BEGIN GENERATED: eventbus-catalogue -->

### `a2a`

| Variante | Charge utile |
|---|---|
| `A2ACompatibilityWarning` | champs nommés |
| `A2AGuardTriggered` | champs nommés |
| `A2AInvocationCompleted` | champs nommés |
| `A2AInvocationStarted` | champs nommés |
| `A2ASkillCompleted` | champs nommés |
| `A2ASkillInvoked` | champs nommés |

### `agent-changed`

| Variante | Charge utile |
|---|---|
| `AgentDegraded` | champs nommés |
| `AgentDisabled` | champs nommés |
| `AgentEnabled` | champs nommés |
| `AgentInstalled` | champs nommés |
| `AgentLoadFailed` | champs nommés |
| `AgentMessageAcked` | champs nommés |
| `AgentMessageDelivered` | champs nommés |
| `AgentMessageDropped` | champs nommés |
| `AgentMessageSent` | champs nommés |
| `AgentReady` | tuple |
| `AgentRegistered` | tuple |
| `AgentStopped` | tuple |
| `AgentStopping` | tuple |
| `AgentUninstalled` | champs nommés |
| `MailboxGuardTriggered` | champs nommés |

### `approval-changed`

| Variante | Charge utile |
|---|---|
| `HitlFilesystemRequired` | champs nommés |
| `PermissionRequired` | champs nommés |
| `TaskApprovalTimeout` | champs nommés |
| `TaskInputRequired` | champs nommés |
| `TaskResumed` | champs nommés |

### `chat-changed`

| Variante | Charge utile |
|---|---|
| `ChatApprovalRequired` | champs nommés |
| `ChatApprovalResolved` | champs nommés |
| `ChatApprovalTimeout` | champs nommés |
| `ChatError` | champs nommés |
| `ChatMessageSent` | champs nommés |
| `ChatResponseCompleted` | champs nommés |
| `ChatResponseStarted` | champs nommés |
| `ChatSessionClosed` | champs nommés |
| `ChatSessionCreated` | champs nommés |
| `ChatToolCallCompleted` | champs nommés |
| `ChatToolCallStarted` | champs nommés |
| `ChatUserInputRequired` | champs nommés |
| `ChatUserInputResolved` | champs nommés |
| `DecisionPointRecorded` | champs nommés |
| `ThinkingEnded` | champs nommés |
| `ThinkingStarted` | champs nommés |
| `ToolCallRetrying` | champs nommés |

### `chat-token`

| Variante | Charge utile |
|---|---|
| `ChatToken` | champs nommés |

### `hook-decision`

| Variante | Charge utile |
|---|---|
| `HookDecisionRecorded` | champs nommés |

### `llm-changed`

| Variante | Charge utile |
|---|---|
| `CostCeilingReached` | champs nommés |
| `LlmCallCompleted` | champs nommés |
| `LlmCallFailed` | champs nommés |
| `LlmFallbackTriggered` | champs nommés |
| `LlmModelFailed` | champs nommés |
| `LlmModelLoading` | champs nommés |
| `LlmModelReady` | champs nommés |
| `LlmResponseCaptured` | champs nommés |
| `MetaLlmBudgetExceeded` | champs nommés |
| `TokenBudgetUpdated` | champs nommés |

### `memory-changed`

| Variante | Charge utile |
|---|---|
| `SharedNamespaceAdded` | champs nommés |

### `onboarding-changed`

| Variante | Charge utile |
|---|---|
| `OnboardingCompleted` | champs nommés |
| `OnboardingRequired` | aucune |
| `OnboardingStarted` | champs nommés |

### `plan-approval`

| Variante | Charge utile |
|---|---|
| `PlanAbandoned` | champs nommés |
| `PlanApprovalRequired` | champs nommés |
| `PlanApproved` | champs nommés |
| `PlanRejected` | champs nommés |

### `plan-mode`

| Variante | Charge utile |
|---|---|
| `ChatPlanApproved` | champs nommés |
| `ChatPlanPhaseChanged` | champs nommés |
| `ChatPlanRejected` | champs nommés |
| `PlanSubmitted` | champs nommés |
| `PlanUpdated` | champs nommés |

### `session-metrics`

| Variante | Charge utile |
|---|---|
| `SessionMetricsUpdated` | champs nommés |

### `stt-changed`

| Variante | Charge utile |
|---|---|
| `SttModelLoaded` | champs nommés |
| `SttRecordingStarted` | aucune |
| `SttRecordingStopped` | champs nommés |
| `SttTranscribed` | champs nommés |
| `SttTranscriptionFailed` | champs nommés |

### `system`

| Variante | Charge utile |
|---|---|
| `AllReady` | aucune |
| `ContextCompacted` | champs nommés |
| `FileModifiedSinceRead` | champs nommés |
| `McpServerHealthChanged` | champs nommés |
| `McpServerReloaded` | champs nommés |
| `ShutdownRequested` | aucune |
| `ToolCircuitBroken` | champs nommés |
| `ToolCircuitRestored` | champs nommés |
| `ToolOutputCaptured` | champs nommés |

### `task-changed`

| Variante | Charge utile |
|---|---|
| `BashFilePathsExtracted` | champs nommés |
| `PlanAlternativesGenerated` | champs nommés |
| `PlanCacheHit` | champs nommés |
| `PlanCompleted` | champs nommés |
| `PlanFailed` | champs nommés |
| `PlanGenerated` | champs nommés |
| `PlanReplanning` | champs nommés |
| `StepCompleted` | champs nommés |
| `StepFailed` | champs nommés |
| `StepStarted` | champs nommés |
| `TaskCanceled` | champs nommés |
| `TaskCompleted` | champs nommés |
| `TaskStarted` | champs nommés |
| `TodoUpdated` | champs nommés |
| `VerificationCompleted` | champs nommés |

### `trace-event`

| Variante | Charge utile |
|---|---|
| `A2AInvokeCompleted` | champs nommés |
| `A2AInvokeStarted` | champs nommés |
| `ActionParseError` | champs nommés |
| `AgentLog` | champs nommés |
| `LlmCallStarted` | champs nommés |
| `Retry` | champs nommés |
| `Thought` | champs nommés |
| `ToolCallCompleted` | champs nommés |
| `ToolCallDenied` | champs nommés |
| `ToolCallStarted` | champs nommés |

### `trigger-fired`

| Variante | Charge utile |
|---|---|
| `TriggerDisabled` | champs nommés |
| `TriggerEnabled` | champs nommés |
| `TriggerError` | champs nommés |
| `TriggerFired` | champs nommés |
| `TriggerQueueFull` | champs nommés |
| `TriggerSkipped` | champs nommés |
| `TriggersReloaded` | champs nommés |
<!-- END GENERATED: eventbus-catalogue -->
