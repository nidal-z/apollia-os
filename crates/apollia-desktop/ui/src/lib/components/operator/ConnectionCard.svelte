<script lang="ts">
  import { X, Sparkles, Plus, Check, Trash2, Settings } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
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
    /**
     * Primary card action - triggered by clicking the card body or pressing
     * Enter/Space when focused. Typical use: open the manage panel for an
     * installed server, or open the install wizard for a catalogue entry.
     */
    onclick?: (e: MouseEvent | KeyboardEvent) => void;
    /**
     * Secondary "uninstall" / "remove" action. When provided, a Trash2 icon
     * appears in the top-right corner on hover (Projects-style affordance).
     * Implementations should typically open a ConfirmDialog before calling
     * the destructive backend command.
     */
    onuninstall?: (e: MouseEvent) => void;
    /**
     * Accessibility label for the card as a whole. Defaults to the name plus
     * a "ouvrir les détails" suffix when `onclick` is wired.
     */
    ariaLabel?: string;
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
    onuninstall,
    ariaLabel,
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

  const computedAriaLabel = $derived(
    ariaLabel ?? (onclick ? `${name} - ouvrir les détails` : name),
  );
</script>

{#if variant === "apollia"}
  <Card
    hover={!!onclick}
    onclick={onclick}
    ariaLabel={computedAriaLabel}
    class="px-4 py-3.5 relative group"
    data-testid="connection-card-apollia"
  >
    <div class="absolute top-2.5 right-2.5 flex items-center gap-1.5">
      {#if onuninstall}
        <Button
          variant="ghost"
          size="icon-sm"
          type="button"
          title="Désinstaller"
          aria-label="Désinstaller {name}"
          onclick={(e: MouseEvent) => {
            e.stopPropagation();
            onuninstall(e);
          }}
          class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 text-muted-foreground hover:bg-destructive/10 hover:text-danger-a11y transition-all"
          data-testid="connection-card-uninstall"
        >
          <Trash2 size={12} strokeWidth={1.75} />
        </Button>
      {/if}
      <Badge variant="primary" size="sm">
        {#snippet icon()}<Sparkles size={9} />{/snippet}
        Apollia
      </Badge>
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
            <span class="truncate flex-1" title={error}>{error}</span>
            {#if onclick}
              <Button
                variant="ghost"
                size="sm"
                type="button"
                onclick={(e: MouseEvent) => {
                  e.stopPropagation();
                  onclick(e);
                }}
                class="ml-auto text-[11px] text-primary font-medium hover:underline"
              >
                Reconnecter →
              </Button>
            {/if}
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
  <Card
    hover={!!onclick}
    onclick={onclick}
    ariaLabel={computedAriaLabel}
    class="px-3.5 py-3 relative group"
    data-testid="connection-card-mcp"
  >
    {#if onuninstall}
      <Button
        variant="ghost"
        size="icon-sm"
        type="button"
        title="Désinstaller"
        aria-label="Désinstaller {name}"
        onclick={(e: MouseEvent) => {
          e.stopPropagation();
          onuninstall(e);
        }}
        class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 text-muted-foreground hover:bg-destructive/10 hover:text-danger-a11y transition-all"
        data-testid="connection-card-uninstall"
      >
        <Trash2 size={12} strokeWidth={1.75} />
      </Button>
    {/if}

    <div class="flex items-center gap-2.5 mb-1 pr-7">
      <div
        class="w-[26px] h-[26px] rounded-md bg-surface-2 text-foreground inline-flex items-center justify-center text-xs font-semibold"
      >
        {initial}
      </div>
      <span class="text-[12.5px] font-semibold text-foreground truncate">{name}</span>
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
      <div class="text-[10.5px] text-muted-foreground mb-2.5 leading-[1.45] line-clamp-2">
        {description}
      </div>
    {/if}
    {#if installed}
      <div
        class="flex items-center justify-between gap-1.5 text-[10.5px] text-success-a11y font-medium"
      >
        <span class="inline-flex items-center gap-1.5">
          <Check size={10} /> Installé
          {#if capabilities !== undefined && capabilities > 0}
            <span class="text-muted-foreground font-normal">· {capabilities} outils</span>
          {/if}
        </span>
        {#if onclick}
          <span
            class="inline-flex items-center gap-1 text-[10.5px] text-primary font-medium opacity-0 group-hover:opacity-100 transition-opacity"
            aria-hidden="true"
          >
            <Settings size={10} /> Gérer
          </span>
        {/if}
      </div>
    {:else if onclick}
      <span
        class="inline-flex items-center gap-1 text-[11px] text-primary font-medium"
        aria-hidden="true"
      >
        <Plus size={10} /> Connecter
      </span>
    {/if}
  </Card>
{/if}
