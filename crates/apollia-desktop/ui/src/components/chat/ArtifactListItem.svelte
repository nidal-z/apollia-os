<script lang="ts">
  import { Code, FileText, Terminal, FileCode, File } from "lucide-svelte";
  import type { Artifact } from "$lib/stores/artifacts";

  interface Props {
    artifact: Artifact;
    active: boolean;
    onselect: () => void;
  }

  let { artifact, active, onselect }: Props = $props();

  const icon = $derived.by(() => {
    switch (artifact.kind) {
      case "code":
        return FileCode;
      case "file":
        return FileText;
      case "bash_output":
        return Terminal;
      case "spec":
        return Code;
      default:
        return File;
    }
  });

  const lineCount = $derived(artifact.content.split("\n").length);
  const createdLabel = $derived.by(() => {
    const d = new Date(artifact.created_at);
    if (Number.isNaN(d.getTime())) return artifact.created_at;
    return d.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  });
</script>

<button
  type="button"
  onclick={onselect}
  data-testid="artifact-list-item"
  class="group flex w-full flex-col gap-1 rounded-md border px-3 py-2 text-left transition-colors
    {active
    ? 'border-primary/40 bg-primary/5'
    : 'border-border/30 bg-transparent hover:border-border/60 hover:bg-muted/20'}"
>
  <div class="flex items-start gap-2">
    <svelte:component
      this={icon}
      size={14}
      class={active ? "mt-0.5 text-primary" : "mt-0.5 text-muted-foreground"}
    />
    <div class="min-w-0 flex-1">
      <p
        class="truncate text-[12px] font-medium leading-snug
          {active ? 'text-primary' : 'text-foreground'}"
        title={artifact.title}
      >
        {artifact.title}
      </p>
      <div
        class="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-muted-foreground/70"
      >
        <span>{artifact.kind}</span>
        {#if artifact.source_tool}
          <span>· {artifact.source_tool}</span>
        {/if}
        <span>· {lineCount} l.</span>
        <span>· {createdLabel}</span>
      </div>
    </div>
  </div>
</button>
