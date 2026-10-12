<script lang="ts">
  /**
   * Says, in a conversation, that a local model is loading: the seconds
   * between the moment an engine starts on a model and the moment it answers.
   */
  import { t } from "svelte-i18n";
  import { Loader2 } from "lucide-svelte";
  import { modelLoads } from "$lib/stores/modelLoads";
</script>

{#each $modelLoads as load (load.engine + load.name)}
  <p class="flex items-center gap-2 px-4 py-2 text-caption text-muted-foreground" role="status">
    <Loader2 size={12} class="animate-spin" aria-hidden="true" />
    {$t("chat.model_loading", { values: { name: load.name } })}
  </p>
{/each}
