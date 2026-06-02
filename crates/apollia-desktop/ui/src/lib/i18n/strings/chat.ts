/** Chat surface: conversation, streaming, plan alternatives, recording overlay. */
export const CHAT_KEYS = {
  root: "chat",
  title: "chat.title",
  newChat: "chat.new_chat",
  emptyTitle: "chat.empty_title",
  emptySubtitle: "chat.empty_subtitle",
  inputPlaceholder: "chat.input_placeholder",
  firstMessagePlaceholder: "chat.first_message_placeholder",
  thinking: "chat.thinking",
  emptyResponseOperator: "chat.empty_response_operator",
  emptyResponseBuilder: "chat.empty_response_builder",
  streamingThought: "chat.streaming_thought",
  recordingHotkeyHint: "chat.recording_hotkey_hint",
  template: {
    brainstorm: {
      title: "chat.template.brainstorm.title",
      description: "chat.template.brainstorm.description",
      prompt: "chat.template.brainstorm.prompt",
    },
    summarize: {
      title: "chat.template.summarize.title",
      description: "chat.template.summarize.description",
      prompt: "chat.template.summarize.prompt",
    },
    draftWriting: {
      title: "chat.template.draft_writing.title",
      description: "chat.template.draft_writing.description",
      prompt: "chat.template.draft_writing.prompt",
    },
    research: {
      title: "chat.template.research.title",
      description: "chat.template.research.description",
      prompt: "chat.template.research.prompt",
    },
    explain: {
      title: "chat.template.explain.title",
      description: "chat.template.explain.description",
      prompt: "chat.template.explain.prompt",
    },
  },
  planAlternatives: {
    choosePlan: "chat.plan_alternatives.choose_plan",
    planALabel: "chat.plan_alternatives.plan_a_label",
    planBLabel: "chat.plan_alternatives.plan_b_label",
    noSteps: "chat.plan_alternatives.no_steps",
    selectedA: "chat.plan_alternatives.selected_a",
    selectedB: "chat.plan_alternatives.selected_b",
    choosePlanA: "chat.plan_alternatives.choose_plan_a",
    choosePlanB: "chat.plan_alternatives.choose_plan_b",
  },
} as const;
