<script lang="ts">
  import { t } from "svelte-i18n";
  import Tooltip from "./Tooltip.svelte";

  interface Props {
    /** The glossary key matching a key in glossary.* i18n namespace */
    term: string;
    /** The visible label text (defaults to the term itself) */
    label?: string;
  }

  let { term, label }: Props = $props();
</script>

<Tooltip content={$t(`glossary.${term}`)} class="max-w-xs">
  <span class="cursor-help border-b border-dotted border-muted-foreground/50">
    {label ?? term}
  </span>
</Tooltip>
