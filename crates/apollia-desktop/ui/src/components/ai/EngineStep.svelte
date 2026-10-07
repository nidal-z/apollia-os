<script lang="ts">
  /**
   * EngineStep - one step of the installation of an engine: its state, its
   * name and a detail in small (the key, the quantities, "not started").
   */
  import { Check, Circle, Loader2, X } from "lucide-svelte";
  import { cn } from "$lib/utils";

  interface Props {
    status: "pending" | "running" | "done" | "refused";
    label: string;
    detail?: string | null;
    testid: string;
  }

  let { status, label, detail = null, testid }: Props = $props();
</script>

<li class="flex items-start gap-2 text-sm" data-testid={testid} data-status={status}>
  <span class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center" aria-hidden="true">
    {#if status === "running"}
      <Loader2 class="h-4 w-4 animate-spin text-primary" />
    {:else if status === "done"}
      <Check class="h-4 w-4 text-success-a11y" />
    {:else if status === "refused"}
      <X class="h-4 w-4 text-danger-a11y" />
    {:else}
      <Circle class="h-3 w-3 text-muted-foreground" />
    {/if}
  </span>
  <span class="min-w-0">
    <span
      class={cn(
        status === "pending" ? "text-muted-foreground" : "text-foreground",
        status === "running" && "font-medium",
      )}
    >
      {label}
    </span>
    {#if detail}
      <span class="block text-xs text-muted-foreground">{detail}</span>
    {/if}
  </span>
</li>
