<script lang="ts">
  /**
   * The one warning shown before a connection is added: what the agents will
   * be able to do with it, where its data goes, what Apollia answers for.
   * Accepting it is the second gesture of the addition, and the desktop
   * journals it; nothing is ticked and nothing is remembered in the window.
   */
  import { t } from "svelte-i18n";
  import { ShieldCheck } from "lucide-svelte";

  interface Props {
    /** Name of the connection, as the selection shows it. */
    name: string;
  }

  let { name }: Props = $props();
</script>

<div
  class="space-y-3 rounded-lg border border-border bg-muted/30 p-4"
  data-testid="wizard-step-disclaimer"
>
  <div class="flex items-center gap-2">
    <ShieldCheck size={16} class="shrink-0 text-primary" aria-hidden="true" />
    <h3 class="text-sm font-semibold text-foreground">
      {$t("integrations.disclaimer.title")}
    </h3>
  </div>
  <p class="text-sm text-foreground">
    {$t("integrations.disclaimer.agents", { values: { name } })}
  </p>
  <p class="text-sm text-foreground">
    {$t("integrations.disclaimer.data", { values: { name } })}
  </p>
  <p class="text-sm text-foreground">
    {$t("integrations.disclaimer.responsibility")}
  </p>
  <p class="text-xs text-muted-foreground" data-testid="disclaimer-journal-note">
    {$t("integrations.disclaimer.journal")}
  </p>
</div>
