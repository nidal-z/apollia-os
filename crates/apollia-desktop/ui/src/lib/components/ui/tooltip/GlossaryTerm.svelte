<script lang="ts">
  import { Tooltip } from "bits-ui";
  import { t } from "svelte-i18n";

  interface Props {
    /** The glossary key matching a key in glossary.* i18n namespace */
    term: string;
    /** The visible label text (defaults to the term itself) */
    label?: string;
  }

  let { term, label }: Props = $props();
</script>

<Tooltip.Root delayDuration={200}>
  <Tooltip.Trigger
    class="cursor-help border-b border-dotted border-muted-foreground/50"
  >
    {label ?? term}
  </Tooltip.Trigger>
  <Tooltip.Portal>
    <Tooltip.Content
      class="z-50 max-w-xs rounded-md glass-card glass-border px-3 py-1.5 text-sm text-card-foreground shadow-md"
      sideOffset={4}
    >
      {$t(`glossary.${term}`)}
    </Tooltip.Content>
  </Tooltip.Portal>
</Tooltip.Root>
