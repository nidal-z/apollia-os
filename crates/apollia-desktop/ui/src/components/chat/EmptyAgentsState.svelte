<script lang="ts">
  /**
   * EmptyAgentsState — shown inside the QuickPicker agents section when
   * nothing is installed (US-SP42-034, B.59).
   *
   * Factored out of `QuickPicker.svelte` so the same surface can be
   * reused elsewhere (context drawer, command palette).
   */
  import { t } from "svelte-i18n";
  import { Bot, BookOpen } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";

  interface Props {
    /** Deep-link target for the documentation link (defaults to wiki). */
    docsHref?: string;
  }

  let {
    docsHref = "https://github.com/nidal-z/apollia-os/wiki/Agents",
  }: Props = $props();
</script>

<div
  class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/50 bg-muted/20 px-4 py-6 text-center"
  role="status"
  aria-live="polite"
  data-testid="empty-agents-state"
>
  <Bot size={18} class="text-muted-foreground/60" aria-hidden="true" />
  <p class="text-[12px] text-muted-foreground">
    {$t("chat.quickpicker.empty_agents")}
  </p>
  <div class="flex gap-2">
    <Button size="sm" onclick={() => navigateTo("agents")}>
      {$t("chat.quickpicker.install_agent")}
    </Button>
    <a
      href={docsHref}
      target="_blank"
      rel="noreferrer"
      class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px]
        text-muted-foreground hover:text-foreground
        focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
    >
      <BookOpen size={11} aria-hidden="true" />
      {$t("chat.quickpicker.docs_link")}
    </a>
  </div>
</div>
