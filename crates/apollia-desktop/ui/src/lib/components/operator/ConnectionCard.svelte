<script lang="ts">
  import { X, Sparkles, Plus, Check } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import Chip from "./Chip.svelte";
  import Card from "./Card.svelte";

  export type ConnectionVariant = "apollia" | "mcp";
  export type ConnectionStatus = "active" | "error" | "syncing" | "idle";

  interface Props {
    variant?: ConnectionVariant;
    name: string;
    /** For Apollia: short tagline. For MCP: vendor (officiel/communauté). */
    vendor?: string;
    description?: string;
    status?: ConnectionStatus;
    /** Number of capabilities/tools exposed. */
    capabilities?: number;
    /** Brand color (hex/CSS) for logo tile. */
    logoColor?: string;
    /** Single-letter logo override (defaults to first char of name). */
    logoChar?: string;
    /** Error message string when status === "error". */
    error?: string;
    /** Last sync label. */
    sync?: string;
    /** MCP only: marked as installed. */
    installed?: boolean;
    /** MCP only: official badge. */
    official?: boolean;
    onclick?: (e: MouseEvent) => void;
  }

  let {
    variant = "apollia",
    name,
    vendor,
    description,
    status = "active",
    capabilities,
    logoColor = "hsl(var(--primary))",
    logoChar,
    error,
    sync,
    installed = false,
    official = false,
    onclick,
  }: Props = $props();

  const initial = $derived(logoChar ?? name.charAt(0).toUpperCase());

  const statusInfo = $derived(
    status === "active"
      ? { color: "hsl(var(--success))", glow: true }
      : status === "error"
        ? { color: "hsl(var(--destructive))", glow: true }
        : status === "syncing"
          ? { color: "hsl(var(--info))", glow: true }
          : { color: "hsl(var(--muted-foreground))", glow: false },
  );
</script>

{#if variant === "apollia"}
  <Card hover class="px-4 py-3.5 relative">
    <div class="absolute top-2.5 right-2.5">
      <Chip tone="primary" size="sm">
        {#snippet icon()}<Sparkles size={9} />{/snippet}
        Apollia
      </Chip>
    </div>
    <div class="flex items-start gap-3">
      <div
        class="w-10 h-10 rounded-[10px] shrink-0 inline-flex items-center justify-center text-white font-semibold text-base"
        style="background: {logoColor};"
      >
        {initial}
      </div>
      <div class="flex-1 min-w-0">
        <div class="flex items-center gap-1.5 mt-px">
          <span class="text-[13px] font-semibold text-foreground">{name}</span>
          <StatusDot color={statusInfo.color} glow={statusInfo.glow} />
        </div>
        {#if description}
          <div class="text-[11px] text-muted-foreground mt-0.5">{description}</div>
        {/if}
        {#if error}
          <div
            class="mt-2 text-[11px] text-danger-a11y flex items-center gap-1.5"
          >
            <X size={11} />
            {error}
            <button
              type="button"
              {onclick}
              class="ml-auto text-[11px] text-primary bg-transparent border-0 cursor-pointer font-medium hover:underline"
            >
              Reconnecter →
            </button>
          </div>
        {:else}
          <div
            class="mt-2 text-[10.5px] text-muted-foreground font-mono flex items-center gap-2"
          >
            {#if capabilities !== undefined}
              <span>{capabilities} outils</span>
              <span>·</span>
            {/if}
            <span>
              {status === "syncing" ? "synchronisation en cours…" : sync ? `sync ${sync}` : ""}
            </span>
          </div>
        {/if}
      </div>
    </div>
  </Card>
{:else}
  <Card hover class="px-3.5 py-3 relative">
    <div class="flex items-center gap-2.5 mb-1">
      <div
        class="w-[26px] h-[26px] rounded-md bg-surface-2 text-foreground inline-flex items-center justify-center text-xs font-semibold"
      >
        {initial}
      </div>
      <span class="text-[12.5px] font-semibold text-foreground">{name}</span>
      {#if official}
        <span
          class="text-[9px] px-[5px] py-px rounded bg-success/10 text-success-a11y font-semibold tracking-[0.3px]"
        >
          ✓ OFFICIEL
        </span>
      {/if}
      {#if vendor && !official}
        <span class="text-[9.5px] text-muted-foreground/70">{vendor}</span>
      {/if}
    </div>
    {#if description}
      <div class="text-[10.5px] text-muted-foreground mb-2.5 leading-[1.45]">
        {description}
      </div>
    {/if}
    {#if installed}
      <div
        class="flex items-center gap-1.5 text-[10.5px] text-success-a11y font-medium"
      >
        <Check size={10} /> Installé
      </div>
    {:else}
      <button
        type="button"
        {onclick}
        class="text-[11px] text-primary bg-transparent border-0 p-0 cursor-pointer font-medium inline-flex items-center gap-1 hover:underline"
      >
        <Plus size={10} /> Connecter
      </button>
    {/if}
  </Card>
{/if}
