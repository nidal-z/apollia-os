<script lang="ts">
  /**
   * Collapsible group of automations owned by a single assistant.
   *
   * Collapsed state is persisted per-agent in localStorage under
   * `apollia.automations.groups.${agentId}.collapsed`. Operator mode
   * defaults to collapsed for every group except the most-recent one
   * (C.T.10) — the parent chooses the default via `defaultOpen`.
   */
  import { onMount } from "svelte";
  import { slide } from "svelte/transition";
  import { prefersReducedMotion } from "$lib/design/motion";
  import { Avatar } from "$lib/components/ui/avatar";
  import { ChevronDown } from "lucide-svelte";
  import type { Snippet } from "svelte";

  interface Props {
    agentName: string;
    count: number;
    defaultOpen?: boolean;
    children: Snippet;
  }

  let { agentName, count, defaultOpen = false, children }: Props = $props();

  const storageKey = $derived(`apollia.automations.groups.${agentName}.collapsed`);

  let open = $state(defaultOpen);
  let hydrated = $state(false);

  onMount(() => {
    try {
      const stored = localStorage.getItem(storageKey);
      if (stored !== null) {
        open = stored === "false"; // "true" means collapsed.
      }
    } catch {
      // localStorage unavailable — keep default.
    }
    hydrated = true;
  });

  function toggle() {
    open = !open;
    try {
      localStorage.setItem(storageKey, String(!open));
    } catch {
      // Ignore quota/permission errors — the UI still toggles.
    }
  }

  const reduced = prefersReducedMotion();
</script>

<section class="rounded-xl border glass-border" data-testid="automation-group-{agentName}">
  <button
    type="button"
    class="group flex w-full items-center justify-between gap-3 rounded-xl px-4 py-3 text-left transition-colors hover:bg-muted/40"
    aria-expanded={open}
    onclick={toggle}
    data-testid="automation-group-toggle-{agentName}"
  >
    <div class="flex items-center gap-2.5">
      <Avatar name={agentName} size="sm" />
      <div>
        <h2 class="text-sm font-semibold text-foreground">{agentName}</h2>
        <p class="text-[11px] text-muted-foreground">{count}</p>
      </div>
    </div>
    <ChevronDown
      size={16}
      strokeWidth={1.75}
      class="text-muted-foreground transition-transform duration-200 {open ? 'rotate-180' : ''}"
      aria-hidden="true"
    />
  </button>

  {#if hydrated && open}
    <div
      class="border-t glass-border px-4 py-4"
      transition:slide={{ duration: reduced ? 0 : 200 }}
    >
      <div class="grid gap-3 md:grid-cols-1 lg:grid-cols-2 xl:grid-cols-3">
        {@render children()}
      </div>
    </div>
  {/if}
</section>
