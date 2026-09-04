<script lang="ts">
  /**
   * EmptyAgentsState - shown inside the QuickPicker agents section when
   * nothing is installed.
   *
   * Factored out of `QuickPicker.svelte` so the same surface can be
   * reused elsewhere (context drawer, command palette).
   */
  import { t, locale } from "svelte-i18n";
  import { Bot, BookOpen } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { docsUrlFor } from "$lib/utils/docsUrl";

  interface Props {
    /**
     * Deep-link target for the documentation link. Left unset, it resolves to
     * the "install an agent" guide under the locale the interface is running
     * in, so a French operator lands on the French page.
     */
    docsHref?: string;
  }

  let { docsHref }: Props = $props();

  const href = $derived(
    docsHref ?? docsUrlFor($locale, "/operator-help/agents/install-an-agent"),
  );
</script>

<div
  class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/50 bg-muted/20 px-4 py-6 text-center"
  role="status"
  aria-live="polite"
  data-testid="empty-agents-state"
>
  <Bot size={18} class="text-muted-foreground/60" aria-hidden="true" />
  <p class="text-body-xs text-muted-foreground">
    {$t("chat.quickpicker.empty_agents")}
  </p>
  <div class="flex gap-2">
    <Button size="sm" onclick={() => navigateTo("agents")}>
      {$t("chat.quickpicker.install_agent")}
    </Button>
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      onclick={handleExternalLinkClick}
      class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-caption
        text-muted-foreground hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
    >
      <BookOpen size={11} aria-hidden="true" />
      {$t("chat.quickpicker.docs_link")}
    </a>
  </div>
</div>
