<script lang="ts">
  /**
   * ChatRulesSection - persistent "Always allow" rules created from the chat.
   *
   * These are agent-scoped rules bound to the Apollia Chat system agent; the
   * brand name stays untranslated by design.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { MessageSquare } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import SettingsSection from "../settings/SettingsSection.svelte";
  import SettingSectionSkeleton from "../settings/SettingSectionSkeleton.svelte";
  import PermissionRuleRow from "./PermissionRuleRow.svelte";
  import PermissionErrorBanner from "./PermissionErrorBanner.svelte";
  import {
    chatPermissionRules,
    loadingChatRules,
    chatRulesError,
    loadChatRules,
    deleteChatRule,
    type PermissionRuleDto,
  } from "$lib/stores/permissions";

  let initialized = $state(false);
  let revokingId = $state<number | null>(null);

  onMount(() => {
    void loadChatRules().finally(() => (initialized = true));
  });

  async function revoke(rule: PermissionRuleDto): Promise<void> {
    revokingId = rule.id;
    try {
      await deleteChatRule(rule.id);
      addToast(
        $t("settings.permissions.toast_chat_deleted", { values: { tool: rule.tool_name } }),
        "success",
        { "data-testid": "chat-permission-revoke-toast" },
      );
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      revokingId = null;
    }
  }
</script>

<SettingsSection title="Apollia Chat" description={$t("settings.permissions.chat_subtitle")} data-testid="permissions-chat">
  {#snippet icon()}<MessageSquare size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    <span class="text-caption text-muted-foreground">
      {$t("settings.permissions.chat_count", { values: { count: $chatPermissionRules.length } })}
    </span>
  {/snippet}

  {#if $chatRulesError}
    <PermissionErrorBanner error={$chatRulesError} onretry={loadChatRules} data-testid="chat-permissions-error" />
  {:else if !initialized && $loadingChatRules}
    <SettingSectionSkeleton />
  {:else if $chatPermissionRules.length === 0}
    <p
      class="rounded-lg border border-dashed border-border px-4 py-4 text-center text-caption text-muted-foreground"
      data-testid="chat-permissions-empty"
    >
      {$t("settings.permissions.chat_empty")}
    </p>
  {:else}
    <ul class="space-y-2" data-testid="chat-permission-rules-list">
      {#each $chatPermissionRules as rule (rule.id)}
        <li animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
          <PermissionRuleRow {rule} busy={revokingId === rule.id} onRevoke={revoke} />
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>
