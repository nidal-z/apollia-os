<script lang="ts">
  /**
   * EmptySessionsState — canonical "no chats yet" surface.
   *
   * Wraps the shared `<EmptyState>` with chat-specific copy and a
   * secondary "Premiers pas" link pointing at the getting-started wiki.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare } from "lucide-svelte";
  import { EmptyState } from "$lib/components/layout";

  interface Props {
    /** Opens the QuickPicker. */
    onnewChat: () => void;
  }

  let { onnewChat }: Props = $props();

  function openGettingStarted(): void {
    window.open(
      "https://github.com/nidal-z/apollia-os/wiki/Premiers-pas",
      "_blank",
      "noreferrer",
    );
  }
</script>

<div data-testid="empty-sessions-state">
  <EmptyState
    icon={MessageSquare}
    title={$t("chat.empty_sessions.title")}
    description={$t("chat.empty_sessions.description")}
    primaryLabel={$t("chat.empty_sessions.new_chat")}
    primaryAction={onnewChat}
    secondaryLabel={$t("chat.empty_sessions.getting_started")}
    secondaryAction={openGettingStarted}
    page="chat-sessions"
  />
</div>
