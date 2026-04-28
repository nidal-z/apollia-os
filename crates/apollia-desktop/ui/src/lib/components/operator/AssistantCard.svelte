<script lang="ts">
  import { Sparkles, Zap } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import Chip from "./Chip.svelte";

  export type AssistantTool = {
    id: string;
    /** Human-friendly description. */
    description?: string;
    /** Permission/scope label (e.g. "fs.read"). */
    permission?: string;
    granted: boolean;
    /** When true, ask each invocation. */
    sensitive?: boolean;
  };

  export type Assistant = {
    name: string;
    /** Kind label shown as chip (e.g. "libre", "tâche", "trigger"). */
    kind?: string;
    description?: string;
    /** When true, dot is success (active). */
    active?: boolean;
    /** Trigger-based, currently idle. */
    idle?: boolean;
    tools?: AssistantTool[];
    /** Number of memory slots used. */
    memorySlots?: number;
    /** List of trigger names attached. */
    triggers?: string[];
    /** Underlying model display label. */
    model?: string;
  };

  interface Props {
    assistant: Assistant;
    /** Compact summary card vs full configuration card. */
    compact?: boolean;
    onToggleTool?: (toolId: string, granted: boolean) => void;
  }

  let { assistant, compact = false, onToggleTool }: Props = $props();

  const toolsCount = $derived(assistant.tools?.length ?? 0);
  const triggersCount = $derived(assistant.triggers?.length ?? 0);
</script>

{#if compact}
  <div
    class="bg-card border border-border rounded-xl px-4 py-3.5"
  >
    <div class="flex items-start gap-2.5 mb-2.5">
      <div
        class="w-[34px] h-[34px] rounded-[9px] shrink-0 inline-flex items-center justify-center"
        style="background: {assistant.idle
          ? 'hsl(var(--muted))'
          : 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)))'}; color: {assistant.idle
          ? 'hsl(var(--muted-foreground))'
          : 'white'}; border: {assistant.idle ? '1px solid hsl(var(--border))' : 'none'};"
      >
        <Sparkles size={15} />
      </div>
      <div class="flex-1 min-w-0">
        <div
          class="text-[13.5px] font-semibold inline-flex items-center gap-1.5 text-foreground"
        >
          {assistant.name}
          {#if assistant.active}
            <StatusDot color="hsl(var(--success))" glow />
          {/if}
          {#if assistant.idle}
            <StatusDot color="hsl(var(--muted-foreground))" />
          {/if}
        </div>
        {#if assistant.description}
          <div class="text-[11px] text-muted-foreground mt-0.5">
            {assistant.description}
          </div>
        {/if}
      </div>
      {#if assistant.kind}
        <Chip size="sm" tone="neutral">{assistant.kind}</Chip>
      {/if}
    </div>
    <div
      class="flex gap-2.5 pt-2.5 border-t border-border/60 text-[10.5px] text-muted-foreground"
    >
      <span>
        <strong class="text-foreground font-semibold">{toolsCount}</strong> outils
      </span>
      <span>
        <strong class="text-foreground font-semibold">{triggersCount}</strong> triggers
      </span>
      <span>
        <strong class="text-foreground font-semibold">{assistant.memorySlots ?? 0}</strong>
        mémoires
      </span>
    </div>
  </div>
{:else}
  <div
    class="bg-card border border-border rounded-xl px-5 py-4"
  >
    <div class="flex items-center gap-3 mb-3.5">
      <div
        class="w-11 h-11 rounded-[11px] inline-flex items-center justify-center shrink-0"
        style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
      >
        <Sparkles size={18} color="white" />
      </div>
      <div class="flex-1 min-w-0">
        <div class="text-base font-semibold text-foreground inline-flex items-center gap-2">
          {assistant.name}
          {#if assistant.active}
            <StatusDot color="hsl(var(--success))" glow />
          {/if}
        </div>
        {#if assistant.description || assistant.model}
          <div class="text-[11.5px] text-muted-foreground">
            {assistant.description ?? ""}
            {#if assistant.model}· {assistant.model}{/if}
          </div>
        {/if}
      </div>
      {#if assistant.kind}
        <Chip size="sm" tone="neutral">{assistant.kind}</Chip>
      {/if}
    </div>
    {#if assistant.tools && assistant.tools.length > 0}
      <div
        class="text-[10.5px] tracking-[1.2px] font-semibold text-muted-foreground uppercase mb-2.5 font-mono"
      >
        Outils activables · {toolsCount}
      </div>
      {#each assistant.tools as t, i}
        <div
          class="flex items-center gap-2.5 py-2.5 {i === assistant.tools.length - 1
            ? ''
            : 'border-b border-border/60'}"
        >
          <div
            class="w-6 h-6 rounded-md inline-flex items-center justify-center {t.granted
              ? 'bg-primary/10 text-primary'
              : 'bg-muted text-muted-foreground'}"
          >
            <Zap size={11} />
          </div>
          <div class="flex-1 min-w-0">
            <div
              class="text-[12.5px] font-medium font-mono {t.granted
                ? 'text-foreground'
                : 'text-muted-foreground'}"
            >
              {t.id}
            </div>
            {#if t.description}
              <div class="text-[10.5px] text-muted-foreground">{t.description}</div>
            {/if}
          </div>
          {#if t.permission}
            <Chip size="sm" tone="neutral">{t.permission}</Chip>
          {/if}
          {#if t.sensitive}
            <Chip size="sm" tone="warning">demander à chaque fois</Chip>
          {/if}
          <button
            type="button"
            onclick={() => onToggleTool?.(t.id, !t.granted)}
            aria-label="Toggle {t.id}"
            class="w-[30px] h-[17px] rounded-full p-0.5 cursor-pointer flex items-center transition-colors {t.granted
              ? 'bg-primary'
              : 'bg-muted'}"
          >
            <div
              class="w-[13px] h-[13px] rounded-full bg-white shadow-elev-1 transition-transform"
              style:transform={t.granted ? "translateX(13px)" : "translateX(0)"}
            ></div>
          </button>
        </div>
      {/each}
    {/if}
  </div>
{/if}
