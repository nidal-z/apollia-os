<script lang="ts">
  /**
   * The right panel of a conversation: what holds for it alone, in two tabs.
   * "Integrations" carries what the agent reaches in this conversation,
   * "Settings" how it answers.
   */
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import type { ChatSessionDetail } from "$lib/types";
  import { SESSION_PANEL_TABS, type SessionPanelTab } from "$lib/chat/sessionSettings";
  import ChatIntegrationsTab from "./ChatIntegrationsTab.svelte";
  import SessionSettingsTab from "./settings/SessionSettingsTab.svelte";

  interface Props {
    session: ChatSessionDetail | null;
    onupdated: () => void;
    /** The tab shown first. */
    initial?: SessionPanelTab;
  }

  let { session, onupdated, initial = "settings" }: Props = $props();

  // svelte-ignore state_referenced_locally
  let tab = $state<SessionPanelTab>(initial);
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="chat-session-panel">
  <div class="border-b border-border px-4 py-3">
    <TabBar
      items={SESSION_PANEL_TABS.map((key) => ({
        key,
        label: $t(`chat.session_panel.${key}`),
      }))}
      activeTab={tab}
      ontabchange={(key) => (tab = key as SessionPanelTab)}
      testidPrefix="chat-session-panel"
    />
  </div>
  <div class="min-h-0 flex-1">
    {#if tab === "integrations"}
      <ChatIntegrationsTab {session} {onupdated} showHeader={false} />
    {:else}
      <SessionSettingsTab {session} />
    {/if}
  </div>
</div>
