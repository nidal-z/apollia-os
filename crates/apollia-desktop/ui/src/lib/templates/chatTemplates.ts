/**
 * Chat session templates.
 *
 * Seeded, typed list of predefined prompts rendered as clickable cards in the
 * QuickPicker. A future community-templates sprint can extend this list via a
 * registry hook — for now, v1 is hard-coded.
 */
import {
  Code2,
  FileText,
  Search,
  Bug,
  Wrench,
  type Icon as LucideIcon,
} from "lucide-svelte";

export interface ChatTemplate {
  /** Stable id — used for analytics & localStorage. */
  id: string;
  /** i18n key for the card title. */
  titleKey: string;
  /** i18n key for the short description. */
  descriptionKey: string;
  /** Prefilled prompt injected into the textarea. */
  prompt: string;
  /** Suggested enabled tools (ids from `tool-catalog`). Empty = defaults. */
  tools?: string[];
  icon: typeof LucideIcon;
}

export const CHAT_TEMPLATES: ChatTemplate[] = [
  {
    id: "dev-review",
    titleKey: "chat.template.dev_review.title",
    descriptionKey: "chat.template.dev_review.description",
    prompt:
      "Review the pending changes on the current branch. Focus on correctness, edge cases, and surface any tests I'm missing.",
    tools: ["file_read", "file_grep", "bash_executor"],
    icon: Code2,
  },
  {
    id: "spec-writing",
    titleKey: "chat.template.spec_writing.title",
    descriptionKey: "chat.template.spec_writing.description",
    prompt:
      "Help me write a technical spec. Ask me clarifying questions first, then produce a structured document with goals, constraints, and open questions.",
    tools: ["file_read", "file_write"],
    icon: FileText,
  },
  {
    id: "research",
    titleKey: "chat.template.research.title",
    descriptionKey: "chat.template.research.description",
    prompt:
      "Research the current state of the art on the topic I'll describe next. Cite sources and flag uncertainty.",
    tools: ["web_search", "web_read"],
    icon: Search,
  },
  {
    id: "debug",
    titleKey: "chat.template.debug.title",
    descriptionKey: "chat.template.debug.description",
    prompt:
      "Help me debug an issue. I'll paste the symptom and the suspected area — narrow it down by asking targeted questions and inspecting the code.",
    tools: ["file_read", "file_grep", "bash_executor"],
    icon: Bug,
  },
  {
    id: "ops-playbook",
    titleKey: "chat.template.ops_playbook.title",
    descriptionKey: "chat.template.ops_playbook.description",
    prompt:
      "Walk me through an operational playbook step by step. Check preconditions, announce each action, and wait for my go-ahead before mutating state.",
    tools: ["bash_executor", "http_fetch"],
    icon: Wrench,
  },
];
