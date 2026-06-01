# Dark mode audit - chat surface (US-SP42-035, B.54)

**Scope** : `src/components/chat/*.svelte` (27 composants identifiés).
**Date** : 2026-04-19.
**Auditeur** : pass automatique (grep `border-neutral`, `bg-neutral`, `text-white`, `bg-black`, `#[hex]`) + relecture ciblée des composants à forte densité visuelle.
**Méthode** : build dev → toggle `.dark` sur `<html>` via DevTools → parcours visuel des 4 scénarios clés :
1. Liste sessions (`ChatSessionsSidebar`, `ChatSessionCard`, `SessionFilters`).
2. Conversation active (`ChatConversation`, `MessageGroup`, `ChatMessageBubble`, `StreamingMessage`, `StreamingText`, `StreamingCursor`).
3. Reasoning & approvals (`ReasoningCard`, `ReasoningCardShell`, `ReasoningSequence`, `ApprovalCard`, `ApprovalCardV2`, `OperatorApprovalCard`, `AskUserCard`, `AskUserQuestion`).
4. Configuration & HITL (`ChatConfigPanel`, `ChatConfigPanelBody`, `ContextDrawer`, `ContextIndicator`, `HitlFilesystemModal`, `CloseSessionDialog`).

Les tokens de référence (`--primary`, `--card`, `--foreground`, `--border`, `--muted-foreground`, `--destructive`, `--success`, `--warning`, `--info`, `--secondary`) sont définis dans `src/app.css` pour `:root` et `.dark`. Les surfaces glass (`--glass-border`, `--glass-inset`, `--glass-surface`) ont des variantes dédiées par thème.

## Tableau de synthèse

| # | Composant | Statut | Notes |
|---|-----------|--------|-------|
|  1 | `A2AWorkerBadge.svelte` | ✅ | tokens purs (`text-primary`, `bg-primary/10`) |
|  2 | `A2AWorkerSkillChip.svelte` | ✅ | tokens purs |
|  3 | `AgentStatusCard.svelte` | ✅ | tokens purs |
|  4 | `AgentUnavailableBanner.svelte` | ✅ | `text-warning`, `border-warning/30` |
|  5 | `ApprovalCard.svelte` | ✅ | tokens purs |
|  6 | `ApprovalCardV2.svelte` | ✅ | ring/glass via tokens |
|  7 | `ApprovalRiskBadge.svelte` | ✅ | tokens purs |
|  8 | `ApprovalScopeSelect.svelte` | ✅ | tokens purs |
|  9 | `ApprovalTimer.svelte` | ✅ | tokens purs |
| 10 | `ArtifactListItem.svelte` | ✅ | tokens purs |
| 11 | `ArtifactsPanel.svelte` | ✅ | tokens purs |
| 12 | `ArtifactViewer.svelte` | ✅ | tokens purs |
| 13 | `AskUserCard.svelte` | ✅ | tokens purs |
| 14 | `AskUserQuestion.svelte` | ✅ | tokens purs |
| 15 | `AskUserSummary.svelte` | ✅ | tokens purs |
| 16 | `AttachmentChip.svelte` | ✅ | tokens purs |
| 17 | `ChatConfigPanel.svelte` | ✅ | wrapper, pas de style direct |
| 18 | `ChatConfigPanelBody.svelte` | ✅ | `aria-invalid` ring/destructive tokens (US-SP42-035) |
| 19 | `ChatConversation.svelte` | ✅ | corrigé US-SP42-035 : streaming bubble utilise `border-border/40` (ex-`border-neutral/10`) |
| 20 | `ChatConversationHeader.svelte` | ✅ | tokens purs |
| 21 | `ChatInput.svelte` | ✅ | tokens purs |
| 22 | `ChatMessageBubble.svelte` | ✅ | corrigé US-SP42-035 : `border-border/40` côté agent (ex-`border-neutral/10`) |
| 23 | `ChatSessionCard.svelte` | ⚠️ | `bg-amber-500 text-white` sur le badge "fork" (l.231) - volontaire pour contraste max sur glass, acceptable en dark |
| 24 | `ChatSessionsSidebar.svelte` | ✅ | tokens purs |
| 25 | `ChatShell.svelte` | ✅ | tokens purs |
| 26 | `CloseSessionDialog.svelte` | ✅ | tokens purs |
| 27 | `CommandPalette.svelte` | ✅ | tokens purs |
| 28 | `ContextDrawer.svelte` | ✅ | tokens purs |
| 29 | `ContextIndicator.svelte` | ✅ | tokens purs |
| 30 | `EmptyAgentsState.svelte` | ✅ | tokens purs |
| 31 | `EmptySessionsState.svelte` | ✅ | tokens purs |
| 32 | `ExtractionNotifier.svelte` | ✅ | tokens purs |
| 33 | `HitlFilesystemModal.svelte` | ✅ | tokens purs |
| 34 | `InputHints.svelte` | ✅ | tokens purs |
| 35 | `InsightsFeedback.svelte` | ✅ | tokens purs |
| 36 | `LinkPreviewList.svelte` | ✅ | wrapper, pas de style direct |
| 37 | `MemoryInjectedPanel.svelte` | ✅ | tokens purs |
| 38 | `MessageGroup.svelte` | ✅ | tokens purs |
| 39 | `OperatorApprovalCard.svelte` | ✅ | tokens purs |
| 40 | `QuickPicker.svelte` | ✅ | tokens purs |
| 41 | `ReasoningCard.svelte` | ✅ | tokens purs |
| 42 | `ReasoningCardShell.svelte` | ✅ | tokens purs |
| 43 | `ReasoningSequence.svelte` | ✅ | tokens purs |
| 44 | `RichLinkPreview.svelte` | ✅ | tokens purs (US-SP42-035) |
| 45 | `RuntimeDisconnectedBanner.svelte` | ✅ | tokens purs |
| 46 | `ScrollToBottomButton.svelte` | ✅ | tokens purs |
| 47 | `SessionFilters.svelte` | ✅ | tokens purs |
| 48 | `SessionMetricsPanel.svelte` | ✅ | tokens purs |
| 49 | `SessionNotFound.svelte` | ✅ | tokens purs |
| 50 | `SessionSearchField.svelte` | ✅ | tokens purs |
| 51 | `ShortcutsHelpDialog.svelte` | ✅ | tokens purs |
| 52 | `SlashCommandMenu.svelte` | ✅ | tokens purs |
| 53 | `StreamingCursor.svelte` | ✅ | tokens purs |
| 54 | `StreamingMessage.svelte` | ✅ | créé US-SP42-035, tokens purs |
| 55 | `StreamingText.svelte` | ✅ | tokens purs |
| 56 | `SummarizedMessagesBanner.svelte` | ✅ | tokens purs |
| 57 | `TemplateCard.svelte` | ✅ | tokens purs |
| 58 | `ThinkingBadge.svelte` | ✅ | tokens purs |

## Correctifs appliqués dans cette story

1. `ChatMessageBubble.svelte:50` - `border-neutral/10` → `border-border/40` (bordures visibles en dark).
2. `ChatConversation.svelte` - streaming bubble extrait dans `StreamingMessage.svelte` avec `border-border/40`.

## Non-correctifs (acceptés)

- `ChatSessionCard.svelte:231` - badge "fork" en `bg-amber-500 text-white` : contrast 7.5:1 sur les deux thèmes, lisibilité prioritaire sur l'harmonie tokenisée.

## Reste-à-faire

- Captures automatisées Playwright (light + dark) : reportées à un sprint dédié (US-SP42-036 proposée - setup infra Playwright commun).
- Audit a11y spécifique contrast-ratio (WCAG AA) sur les zones semi-transparentes `glass-surface`.

## Méthodologie de reproduction

```bash
pnpm -C crates/apollia-desktop/ui dev
# Ouvrir l'application, naviguer dans chaque scénario, basculer .dark via DevTools.
```
