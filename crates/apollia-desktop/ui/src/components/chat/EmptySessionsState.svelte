<script lang="ts">
  /**
   * EmptySessionsState - canonical "no chats yet" surface.
   *
   * Wraps the shared `<EmptyState>` with chat-specific copy and a
   * secondary "first steps" link pointing at the published chat guide, in the
   * locale the interface is running in.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare } from "lucide-svelte";
  import { EmptyState } from "$lib/components/layout";
  import { openExternalUrl } from "$lib/utils/externalLink";
  import { docsUrl } from "$lib/utils/docsUrl";

  interface Props {
    /** Opens the QuickPicker. */
    onnewChat: () => void;
  }

  let { onnewChat }: Props = $props();

  function openGettingStarted(): void {
    void openExternalUrl(docsUrl("/operator-help/chat/chat-with-your-ai"));
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
