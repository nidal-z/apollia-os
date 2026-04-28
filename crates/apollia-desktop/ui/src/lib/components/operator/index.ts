// Operator design-system components — extracted from V3 design canvas.
// Primitives
export { default as PageHeader } from "./PageHeader.svelte";
export { default as SectionTitle } from "./SectionTitle.svelte";
export { default as BtnPrimary } from "./BtnPrimary.svelte";
export { default as BtnSecondary } from "./BtnSecondary.svelte";
export { default as Chip } from "./Chip.svelte";
export { default as StatusDot } from "./StatusDot.svelte";
export { default as Card } from "./Card.svelte";
export { default as EmptyState } from "./EmptyState.svelte";
export { default as Kbd } from "./Kbd.svelte";

// Domain components
export { default as ProjectCard } from "./ProjectCard.svelte";
export { default as ConnectionCard } from "./ConnectionCard.svelte";
export { default as HITLCard } from "./HITLCard.svelte";
export { default as Composer } from "./Composer.svelte";
export { default as Journal } from "./Journal.svelte";
export { default as TaskRow } from "./TaskRow.svelte";
export { default as InboxRow } from "./InboxRow.svelte";
export { default as AssistantCard } from "./AssistantCard.svelte";
export { default as ConversationRow } from "./ConversationRow.svelte";
export { default as NewProjectDialog } from "./NewProjectDialog.svelte";
export { default as TriggerCard } from "./TriggerCard.svelte";
export {
  default as TriggerSourcePicker,
  TRIGGER_SOURCES,
  CRON_PRESETS,
} from "./TriggerSourcePicker.svelte";
export { default as EmptyStates } from "./EmptyStates.svelte";

// Re-exported types
export type { ProjectStatus } from "./ProjectCard.svelte";
export type { ConnectionVariant, ConnectionStatus } from "./ConnectionCard.svelte";
export type { RiskLevel } from "./HITLCard.svelte";
export type {
  ComposerState,
  Attachment,
  SlashCmd,
  Mention,
} from "./Composer.svelte";
export type { JournalEvent, JournalEventType, JournalMode } from "./Journal.svelte";
export type { Task, TaskStatus } from "./TaskRow.svelte";
export type { InboxType } from "./InboxRow.svelte";
export type {
  Assistant,
  AssistantTool,
} from "./AssistantCard.svelte";
export type { ConversationState } from "./ConversationRow.svelte";
export type { ProjectTemplate } from "./NewProjectDialog.svelte";
export type {
  Trigger,
  TriggerSource,
  TriggerStatus,
} from "./TriggerCard.svelte";
export type {
  TriggerSourceKind,
  TriggerSourceDef,
  CronPreset,
} from "./TriggerSourcePicker.svelte";
