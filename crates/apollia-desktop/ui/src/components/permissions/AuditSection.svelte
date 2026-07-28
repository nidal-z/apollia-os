<script lang="ts">
  /**
   * AuditSection - read-only tail of the immutable permission-decision log.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { ScrollText } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import SettingsSection from "../settings/SettingsSection.svelte";
  import PermissionErrorBanner from "./PermissionErrorBanner.svelte";
  import { auditEntries, auditError, loadAudit } from "$lib/stores/permissions";

  onMount(() => {
    void loadAudit();
  });

  function formatTime(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toISOString().slice(11, 16);
  }

  function decisionClass(decision: string): string {
    if (decision === "allow") return "text-success";
    if (decision === "deny") return "text-destructive";
    return "text-muted-foreground";
  }
</script>

<SettingsSection title={$t("settings.permissions.audit_title")} data-testid="permissions-audit">
  {#snippet icon()}<ScrollText size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    <Badge variant="neutral">{$t("settings.permissions.audit_readonly")}</Badge>
  {/snippet}

  {#if $auditError}
    <PermissionErrorBanner error={$auditError} onretry={() => loadAudit()} />
  {:else if $auditEntries.length === 0}
    <p class="text-caption text-muted-foreground">{$t("settings.permissions.audit_empty")}</p>
  {:else}
    <ul class="divide-y divide-border/60 overflow-hidden rounded-lg border border-border/60 text-caption">
      {#each $auditEntries.slice(0, 20) as entry (entry.id)}
        <li class="grid grid-cols-[3rem_8rem_1fr_8rem] items-center gap-2 px-3 py-1.5">
          <span class="font-mono text-muted-foreground tabular-nums">{formatTime(entry.decided_at)}</span>
          <span class="font-medium">{entry.tool_name}</span>
          <span class={decisionClass(entry.decision)}>
            {entry.decision}{#if entry.scope}<span class="text-muted-foreground"> · {entry.scope}</span>{/if}{#if entry.rule_id}<span class="text-muted-foreground"> · {$t("settings.permissions.audit_rule_ref", { values: { id: entry.rule_id } })}</span>{/if}
          </span>
          <span class="text-right text-muted-foreground">{entry.agent ?? "-"}</span>
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>
