<script lang="ts">
  /**
   * CapabilityMatrix - the "what Apollia can do" tab for a native connector.
   *
   * Frames the free-tier scope policy up front, then lists each capability
   * group with a supported/unsupported mark. Builder mode additionally exposes
   * the raw granted OAuth scopes.
   */
  import { t } from "svelte-i18n";
  import { Check, X } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { capabilityGroupsFor, type ProviderId } from "$lib/connections/capabilities";

  interface Props {
    providerId: ProviderId;
  }

  let { providerId }: Props = $props();

  const groups = $derived(capabilityGroupsFor(providerId));
</script>

<div class="max-w-3xl space-y-4" data-testid="native-capabilities">
  <Card class="border-primary/30 bg-primary/5 px-4 py-3.5">
    <div class="mb-2 text-overline text-primary/80">
      {$t("connections.capabilities.scope_policy_title")}
    </div>
    <p class="text-body-sm text-foreground/85">
      {$t(`connections.capabilities.scope_policy_body_${providerId}`)}
    </p>
  </Card>

  {#each groups as group (group.key)}
    <Card class="px-4 py-3.5">
      <div class="mb-2 flex items-center gap-2 text-label-sm font-semibold text-foreground">
        <span>{$t(`connections.capabilities.group_${group.key}`)}</span>
      </div>
      <ul class="space-y-1.5">
        {#each group.entries as entry (entry.labelKey)}
          <li class="flex items-start gap-2.5 text-body-sm">
            <span
              class="mt-0.5 inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full {entry.supported
                ? 'bg-success/20 text-success'
                : 'bg-destructive/15 text-destructive'}"
            >
              {#if entry.supported}<Check size={9} strokeWidth={3} />{:else}<X size={9} strokeWidth={3} />{/if}
            </span>
            <div class="flex-1">
              <div class="text-foreground/90">{$t(entry.labelKey)}</div>
              {#if entry.detailKey}
                <div class="mt-0.5 text-caption text-muted-foreground">{$t(entry.detailKey)}</div>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    </Card>
  {/each}

  <Card class="bg-muted/30 px-4 py-3.5">
    <p class="text-caption text-muted-foreground">
      {$t("connections.capabilities.expert_mode_hint")}
    </p>
  </Card>
</div>
