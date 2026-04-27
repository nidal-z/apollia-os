<script lang="ts">
  /**
   * Clickable template card for the QuickPicker.
   */
  import { t } from "svelte-i18n";
  import type { ChatTemplate } from "$lib/templates/chatTemplates";

  interface Props {
    template: ChatTemplate;
    disabled?: boolean;
    onselect: (template: ChatTemplate) => void;
  }

  let { template, disabled = false, onselect }: Props = $props();
  const Icon = $derived(template.icon);
</script>

<button
  type="button"
  class="flex w-full flex-col gap-1.5 rounded-lg border border-border/40 bg-card/60 px-3 py-2.5 text-left
    transition-all hover:border-primary/40 hover:bg-primary/5 active:scale-[0.99]
    focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40
    disabled:opacity-50 disabled:pointer-events-none"
  onclick={() => onselect(template)}
  {disabled}
  data-testid="quickpicker-template-{template.id}"
>
  <div class="flex items-center gap-2">
    <span class="text-primary/80" aria-hidden="true"><Icon size={14} /></span>
    <span class="flex-1 truncate text-[12px] font-medium text-foreground">
      {$t(template.titleKey)}
    </span>
  </div>
  <p class="line-clamp-2 text-[11px] text-muted-foreground/80">
    {$t(template.descriptionKey)}
  </p>
</button>
