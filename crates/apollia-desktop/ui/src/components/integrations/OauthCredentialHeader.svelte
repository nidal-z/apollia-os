<script lang="ts">
  /**
   * OauthCredentialHeader - the label row of an OauthCredentialRow: the field
   * icon and label, a required/optional tag, and the state indicator. Public
   * (client_id) rows show the resolution `source` pill; secret rows show a
   * configured / absent badge.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, AlertTriangle, Cog } from "lucide-svelte";
  import type { OauthSource } from "$lib/ipc/oauthClients";

  interface Props {
    label: string;
    icon: Snippet;
    required: boolean;
    mode: "public" | "secret";
    source: OauthSource;
    configured: boolean;
  }

  let { label, icon, required, mode, source, configured }: Props = $props();

  const sourceTone = $derived(
    source === "env"
      ? "border-warning/35 bg-warning/5 text-warning-a11y"
      : source === "file"
        ? "border-primary/35 bg-primary/5 text-primary"
        : source === "builtin"
          ? "border-success/35 bg-success/5 text-success-a11y"
          : "border-destructive/30 bg-destructive/5 text-danger-a11y",
  );
</script>

<div class="mb-2 flex flex-wrap items-center justify-between gap-2">
  <span class="inline-flex items-center gap-2 text-label-md text-foreground">
    <span class="text-muted-foreground">{@render icon()}</span>
    {label}
    <span
      class="rounded px-1.5 py-px text-overline uppercase text-muted-foreground {required
        ? 'bg-muted'
        : 'border border-border/60'}"
    >
      {required
        ? $t("settings.integrations.required")
        : $t("settings.integrations.optional")}
    </span>
  </span>

  {#if mode === "public"}
    <span
      class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-caption font-medium {sourceTone}"
    >
      <Cog size={11} strokeWidth={2} aria-hidden="true" />
      {$t(`settings.integrations.source.${source}`)}
    </span>
  {:else if configured}
    <span
      class="inline-flex items-center gap-1.5 rounded-full bg-success/12 px-2 py-0.5 text-caption font-medium text-success-a11y"
    >
      <Check size={11} strokeWidth={2.5} aria-hidden="true" />
      {$t("settings.credential.configured")}
    </span>
  {:else}
    <span
      class="inline-flex items-center gap-1.5 rounded-full bg-warning/12 px-2 py-0.5 text-caption font-medium text-warning-a11y"
    >
      <AlertTriangle size={11} strokeWidth={2} aria-hidden="true" />
      {$t("settings.credential.absent")}
    </span>
  {/if}
</div>
