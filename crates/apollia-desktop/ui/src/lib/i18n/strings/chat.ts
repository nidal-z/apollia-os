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
  streamingThought: "chat.streaming_thought",
  recordingHotkeyHint: "chat.recording_hotkey_hint",
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
