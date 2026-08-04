<script lang="ts">
  /**
   * SessionAuthSection - the "For this session" tool authorizations.
   *
   * These are scoped to a chat session rather than to a rule, and revoking one
   * here removes it from the running session immediately. They are not
   * ephemeral, despite the wording that used to sit here: the runtime persists
   * them to `chat_tool_authorizations` and `restore_sessions` reads them back
   * into each active session at boot, so they survive a restart and a seeded
   * profile shows them on first launch.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { Clock } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import SettingsSection from "../settings/SettingsSection.svelte";
  import SettingSectionSkeleton from "../settings/SettingSectionSkeleton.svelte";
  import PermissionErrorBanner from "./PermissionErrorBanner.svelte";
  import {
    sessionAuthorizations,
    loadingSessionAuths,
    sessionAuthsError,
    loadSessionAuthorizations,
    revokeSessionAuthorization,
    type SessionAuthorizationDto,
  } from "$lib/stores/permissions";

  let initialized = $state(false);
  let revokingKey = $state<string | null>(null);

  onMount(() => {
    void loadSessionAuthorizations().finally(() => (initialized = true));
  });

  function modeLabel(mode: string): string {
    if (mode === "libre") return "Apollia Chat";
    if (mode === "agent") return $t("settings.permissions.scope_agent");
    if (mode === "companion") return "Companion";
    return mode;
  }

  async function revoke(entry: SessionAuthorizationDto): Promise<void> {
    const key = `${entry.session_id}::${entry.tool_name}`;
    revokingKey = key;
    try {
      await revokeSessionAuthorization(entry.session_id, entry.tool_name);
      addToast(
        $t("settings.permissions.toast_session_revoked", { values: { tool: entry.tool_name } }),
        "success",
        { "data-testid": "session-auth-revoke-toast" },
      );
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      revokingKey = null;
    }
  }
</script>

<SettingsSection
  title={$t("settings.permissions.sessions_title")}
  description={$t("settings.permissions.sessions_subtitle")}
  data-testid="permissions-session-auths"
>
  {#snippet icon()}<Clock size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    <span class="text-caption text-muted-foreground">
      {$t("settings.permissions.sessions_count", { values: { count: $sessionAuthorizations.length } })}
    </span>
  {/snippet}

  {#if $sessionAuthsError}
    <PermissionErrorBanner
      error={$sessionAuthsError}
      onretry={loadSessionAuthorizations}
      data-testid="session-auths-error"
    />
  {:else if !initialized && $loadingSessionAuths}
    <SettingSectionSkeleton />
  {:else if $sessionAuthorizations.length === 0}
    <p
      class="rounded-lg border border-dashed border-border px-4 py-4 text-center text-caption text-muted-foreground"
      data-testid="session-auths-empty"
    >
      {$t("settings.permissions.sessions_empty")}
    </p>
  {:else}
    <ul class="space-y-2" data-testid="session-auths-list">
      {#each $sessionAuthorizations as entry (entry.session_id + "::" + entry.tool_name)}
        {@const key = entry.session_id + "::" + entry.tool_name}
        <li
          animate:flip={listFlip()}
          in:fly={rowIn()}
          out:fly={rowOut()}
          class="flex items-start justify-between gap-3 rounded-lg border border-border bg-card px-3.5 py-3 shadow-sm"
          data-testid="session-auth-row"
          data-session-id={entry.session_id}
          data-tool-name={entry.tool_name}
        >
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <code class="font-mono text-body-xs text-foreground">{entry.tool_name}</code>
              <Badge variant="warning">{$t("settings.permissions.session_badge")}</Badge>
            </div>
            <p class="mt-0.5 text-caption text-muted-foreground">
              {modeLabel(entry.mode)} · {entry.session_title ?? entry.session_id.slice(0, 8)}
            </p>
            <p class="mt-0.5 text-caption italic text-muted-foreground">
              {$t("settings.permissions.session_expire_hint")}
            </p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 px-2 text-caption"
            disabled={revokingKey === key}
            onclick={() => revoke(entry)}
            data-testid="session-auth-revoke"
          >
            {$t("permissions.rules.revoke")}
          </Button>
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>
