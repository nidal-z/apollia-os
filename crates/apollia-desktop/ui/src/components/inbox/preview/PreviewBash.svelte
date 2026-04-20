<script lang="ts">
  /**
   * Bash command preview (US-SP42-054). Shows dry-run output when the
   * command supports `--dry-run`; otherwise falls back to static analysis
   * (detected commands + shellcheck warnings) and warns the operator
   * that real execution is required.
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle, Terminal } from "lucide-svelte";
  import type { BashPreview } from "./types";

  interface Props {
    payload: BashPreview;
  }

  let { payload }: Props = $props();
</script>

<div class="space-y-3" data-testid="preview-bash">
  <section>
    <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
      {$t("permissions.preview.bash.command")}
    </h4>
    <pre class="overflow-x-auto rounded bg-muted px-3 py-2 text-[12px]"><code>{payload.command}</code></pre>
  </section>

  {#if !payload.dry_run_supported}
    <div class="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
      <AlertTriangle size={14} class="mt-0.5 shrink-0 text-warning" />
      <div>
        <p class="font-medium">{$t("permissions.preview.bash.no_dry_run_title")}</p>
        <p class="text-muted-foreground">{$t("permissions.preview.bash.no_dry_run_body")}</p>
      </div>
    </div>
  {/if}

  {#if payload.dry_run_output}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.bash.dry_run_output")}
      </h4>
      <pre class="max-h-[320px] overflow-auto rounded bg-muted px-3 py-2 text-[11px]"
        data-testid="preview-bash-dry-run"
      >{payload.dry_run_output}</pre>
    </section>
  {/if}

  {#if payload.detected_commands?.length}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.bash.detected")}
      </h4>
      <ul class="space-y-1 text-[12px]">
        {#each payload.detected_commands as cmd (cmd)}
          <li class="flex items-center gap-2 font-mono">
            <Terminal size={12} class="text-muted-foreground" />
            {cmd}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if payload.warnings?.length}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.bash.warnings")}
      </h4>
      <ul class="space-y-1 text-[12px]">
        {#each payload.warnings as w, i (i)}
          <li
            class="rounded border px-2 py-1"
            class:border-destructive={w.severity === "error"}
            class:border-warning={w.severity === "warning"}
            class:border-border={w.severity === "info"}
          >
            <span class="font-semibold uppercase tracking-wide">{w.severity}</span>
            <span class="ml-2">{w.message}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>
