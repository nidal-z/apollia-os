/**
 * Chat session templates.
 *
 * Seeded list of ultra-generic prompts rendered as clickable cards in the
 * QuickPicker. Use cases are intentionally kept domain-agnostic so the
 * gallery is useful for any operator profile (writer, manager, researcher,
 * developer…). A future community-templates sprint can extend this list.
 */
import {
  Lightbulb,
  FileText,
  PenLine,
  Search,
  GraduationCap,
  type Icon as LucideIcon,
} from "lucide-svelte";

export interface ChatTemplate {
  /** Stable id — used for analytics & localStorage. */
  id: string;
  /** i18n key for the card title. */
  titleKey: string;
  /** i18n key for the short description. */
  descriptionKey: string;
  /** i18n key for the prefilled prompt injected into the textarea. */
  promptKey: string;
  /** Suggested enabled tools (ids from `tool-catalog`). Empty = defaults. */
  tools?: string[];
  icon: typeof LucideIcon;
}

export const CHAT_TEMPLATES: ChatTemplate[] = [
  {
    id: "brainstorm",
    titleKey: "chat.template.brainstorm.title",
    descriptionKey: "chat.template.brainstorm.description",
    promptKey: "chat.template.brainstorm.prompt",
    icon: Lightbulb,
  },
  {
    id: "summarize",
    titleKey: "chat.template.summarize.title",
    descriptionKey: "chat.template.summarize.description",
    promptKey: "chat.template.summarize.prompt",
    icon: FileText,
  },
  {
    id: "draft-writing",
    titleKey: "chat.template.draft_writing.title",
    descriptionKey: "chat.template.draft_writing.description",
    promptKey: "chat.template.draft_writing.prompt",
    icon: PenLine,
  },
  {
    id: "research",
    titleKey: "chat.template.research.title",
    descriptionKey: "chat.template.research.description",
    promptKey: "chat.template.research.prompt",
    icon: Search,
  },
  {
    id: "explain",
    titleKey: "chat.template.explain.title",
    descriptionKey: "chat.template.explain.description",
    promptKey: "chat.template.explain.prompt",
    icon: GraduationCap,
  },
];
