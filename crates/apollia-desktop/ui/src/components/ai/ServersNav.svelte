<script lang="ts">
  /**
   * ServersNav - the "Servers" group of the left column of the AI screen: one
   * entry per saved server, or the entry that says none is connected, then
   * the entry that adds one.
   */
  import { t } from "svelte-i18n";
  import { Plus, Server } from "lucide-svelte";
  import type { DeclaredServer } from "$lib/ipc/llmServers";
  import { cn } from "$lib/utils";
  import type { ServerModelsState } from "./servers";

  interface Props {
    servers: readonly DeclaredServer[];
    models: Readonly<Record<string, ServerModelsState>>;
    /** Name of the server shown in the content, `null` when none is. */
    selected: string | null;
    /** Whether the content shows the form of a new server. */
    adding: boolean;
    /** Whether the content says that no server is connected yet. */
    none: boolean;
    onselect: (name: string) => void;
    onadd: () => void;
    /** Opens what a server brings, offered while none is connected. */
    onnone: () => void;
  }

  let { servers, models, selected, adding, none, onselect, onadd, onnone }: Props = $props();

  const ENTRY =
    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60";
  const ACTIVE = "bg-muted font-medium text-foreground";
  const IDLE = "text-muted-foreground hover:bg-muted/60 hover:text-foreground";
</script>

<div class="mt-4 flex flex-col gap-0.5" data-testid="ai-servers-nav">
  <p class="px-2 pb-2 text-xs text-muted-foreground">{$t("ai.servers.title")}</p>
  {#each servers as server (server.name)}
    {@const state = models[server.name]}
    <button
      type="button"
      class={cn(ENTRY, selected === server.name ? ACTIVE : IDLE)}
      aria-current={selected === server.name ? "page" : undefined}
      onclick={() => onselect(server.name)}
      data-testid="ai-server-entry"
      data-server={server.name}
    >
      <Server size={14} strokeWidth={1.75} aria-hidden="true" />
      <span class="min-w-0 flex-1 truncate">{server.name}</span>
      {#if server.enabled && state?.phase === "listed"}
        <span class="text-xs tabular-nums text-muted-foreground" data-testid="ai-server-entry-count">
          {state.models.length}
        </span>
      {/if}
    </button>
  {/each}
  {#if servers.length === 0}
    <button
      type="button"
      class={cn(ENTRY, none ? ACTIVE : IDLE)}
      aria-current={none ? "page" : undefined}
      onclick={onnone}
      data-testid="ai-servers-none"
    >
      <Server size={14} strokeWidth={1.75} aria-hidden="true" />
      <span class="min-w-0 flex-1 truncate">{$t("ai.servers.empty_title")}</span>
    </button>
  {/if}
  <button
    type="button"
    class={cn(ENTRY, adding ? ACTIVE : IDLE)}
    aria-current={adding ? "page" : undefined}
    onclick={onadd}
    data-testid="ai-server-add"
  >
    <Plus size={14} strokeWidth={1.75} aria-hidden="true" />
    <span>{$t("ai.servers.add")}</span>
  </button>
</div>
