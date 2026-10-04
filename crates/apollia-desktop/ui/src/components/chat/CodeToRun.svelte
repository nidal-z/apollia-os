<script lang="ts">
  /**
   * The code a consent card asks about, whole: the command or the source as it
   * will run, never cut, with the folder it runs in. A line wider than the card
   * scrolls sideways rather than wrapping, so what is read is what runs.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    /** Prefix of every `data-testid`, completed by `-{part}-{toolName}`. */
    testidPrefix: string;
    toolName: string;
    code: string;
    workingDir: string | null;
  }

  let { testidPrefix, toolName, code, workingDir }: Props = $props();
</script>

<div class="mt-2" data-testid="{testidPrefix}-code-card-{toolName}">
  <Badge variant="warning" size="sm">{$t("approval.code.family")}</Badge>

  {#if workingDir}
    <p class="mt-2 text-caption text-foreground" data-testid="{testidPrefix}-workdir-{toolName}">
      {$t("approval.code.working_dir", { values: { dir: workingDir } })}
    </p>
  {/if}

  <pre
    class="mt-2 max-h-72 overflow-auto whitespace-pre rounded-lg bg-surface-2 p-2 font-mono text-micro text-foreground"
    data-testid="{testidPrefix}-code-{toolName}">{code}</pre>
</div>
