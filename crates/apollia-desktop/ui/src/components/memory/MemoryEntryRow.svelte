<script lang="ts">
  import { Clock, Database, Cog, MoreHorizontal, Trash2, Eye, Copy, Check, X } from "lucide-svelte";
  import type { MemoryEntry } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { ActionMenu } from "$lib/components/ui/action-menu";

  interface Props {
    entry: MemoryEntry;
    /** True quand cette row est sélectionnée (panneau détail ouvert sur elle). */
    selected?: boolean;
    /** True en mode résultat de recherche : affiche le score BM25 si présent. */
    searching?: boolean;
    onclick?: () => void;
    oncopy?: () => void;
    ondelete?: () => void;
  }

  let { entry, selected = false, searching = false, onclick, oncopy, ondelete }: Props = $props();

  // ── Catégorisation visuelle des clés (préfixes système) ─────────────────────
  // Le but est de différencier visuellement les clés "système" (`seen:`,
  // `entity:*`, `bootstrap.*`, `procedure:*`) des clés métier libres, sans
  // ajouter de couleur - juste une icône + un léger styling de mono.
  type KeyKind = "seen" | "entity" | "bootstrap" | "procedure" | "user" | "default";
  const keyKind: KeyKind = $derived.by(() => {
    if (entry.key.startsWith("seen:")) return "seen";
    if (entry.key.startsWith("entity:")) return "entity";
    if (entry.key.startsWith("bootstrap.")) return "bootstrap";
    if (entry.key.startsWith("procedure:")) return "procedure";
    if (entry.key.startsWith("user.")) return "user";
    return "default";
  });

  // Icone par TYPE d'entrée - rendue explicitement via if/else dans le markup
  // (volontairement uniforme en couleur muted-foreground pour éviter la collision
  // tonale avec les chips de catégorie utilisateur).

  const typeLabel = $derived(
    entry.entry_type === "episodic" ? "Épisodique" :
    entry.entry_type === "semantic" ? "Sémantique" :
    entry.entry_type === "procedural" ? "Procédurale" : entry.entry_type
  );

  // ── Preview tronquée (sans pretty-print, le détail est dans le Sheet) ────────
  const valuePreview = $derived.by(() => {
    if (!entry.value) return "";
    // Si JSON, prendre la 1re valeur exploitable comme aperçu
    let v = entry.value.trim();
    if (v.startsWith('"') && v.endsWith('"')) v = v.slice(1, -1);
    if (v.length > 120) v = v.slice(0, 120) + "…";
    return v;
  });

  // ── Temps relatif court ──────────────────────────────────────────────────────
  function relativeTime(iso: string): string {
    const now = Date.now();
    const then = new Date(iso).getTime();
    const diffMs = now - then;
    if (diffMs < 0) return "à venir";
    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}min`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h`;
    const days = Math.floor(hours / 24);
    if (days < 7) return `${days}j`;
    const weeks = Math.floor(days / 7);
    if (weeks < 5) return `${weeks}sem`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months}mo`;
    return `${Math.floor(days / 365)}an`;
  }

  function ttlBadge(expiresAt: string | null): string {
    if (expiresAt === null) return "";
    const diffMs = new Date(expiresAt).getTime() - Date.now();
    if (diffMs <= 0) return "expiré";
    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours < 24) return `TTL ${hours}h`;
    const days = Math.floor(hours / 24);
    return `TTL ${days}j`;
  }

  // ── Menu kebab + confirmation suppression inline (pattern ConversationRow) ──
  const hasActions = $derived(Boolean(oncopy || ondelete));
  let confirmingDelete = $state(false);

  function onRowClick(ev: MouseEvent): void {
    const target = ev.target as HTMLElement | null;
    if (target?.closest("[data-memory-row-actions]")) return;
    onclick?.();
  }

  function onRowKeydown(ev: KeyboardEvent): void {
    if (ev.key !== "Enter" && ev.key !== " ") return;
    const target = ev.target as HTMLElement | null;
    if (target?.closest("[data-memory-row-actions]")) return;
    ev.preventDefault();
    onclick?.();
  }

  // Score BM25 affichage formaté
  const scoreLabel = $derived(
    searching && entry.score !== null && entry.score !== undefined
      ? entry.score.toFixed(2)
      : ""
  );
</script>

<div
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  onclick={onclick ? onRowClick : undefined}
  onkeydown={onclick ? onRowKeydown : undefined}
  class="group relative flex items-start gap-3 px-6 py-3.5 cursor-pointer border-b border-border/40 transition-colors {selected
    ? 'bg-primary/10'
    : 'bg-transparent hover:bg-muted/40'}"
  data-testid="memory-entry-row-{entry.id}"
>
  <!-- Icone type - uniforme en gris muted, pas de couleur sémantique
       pour éviter la collision tonale avec les chips de catégorie -->
  <div
    class="w-[28px] h-[28px] rounded-md shrink-0 mt-0.5 inline-flex items-center justify-center {selected
      ? 'bg-primary text-white'
      : 'bg-muted text-muted-foreground'}"
    title={typeLabel}
  >
    {#if entry.entry_type === "episodic"}
      <Clock size={13} />
    {:else if entry.entry_type === "procedural"}
      <Cog size={13} />
    {:else}
      <Database size={13} />
    {/if}
  </div>

  <div class="flex-1 min-w-0">
    <!-- Ligne 1 : key + badges -->
    <div class="flex items-center gap-2 min-w-0">
      <code
        class="text-[13px] truncate {keyKind === 'seen'
          ? 'text-muted-foreground/80'
          : keyKind === 'entity'
            ? 'text-foreground'
            : keyKind === 'bootstrap'
              ? 'text-muted-foreground'
              : keyKind === 'procedure'
                ? 'text-muted-foreground'
                : keyKind === 'user'
                  ? 'text-foreground'
                  : 'text-foreground'}"
        style:font-weight={selected ? 600 : 500}
        title={entry.key}
      >
        {entry.key}
      </code>

      <!-- Type badge minimal (outline, neutre) à droite de la key -->
      <span
        class="inline-flex shrink-0 items-center px-1.5 py-px rounded text-[9.5px] font-medium tabular-nums tracking-tight border border-border/60 bg-background text-muted-foreground"
      >
        {typeLabel.slice(0, 4).toLowerCase()}
      </span>

      {#if scoreLabel}
        <span
          class="inline-flex shrink-0 items-center px-1.5 py-px rounded text-[9.5px] font-mono tabular-nums tracking-tight bg-info/10 text-info"
          title="Score BM25"
        >
          {scoreLabel}
        </span>
      {/if}
    </div>

    <!-- Ligne 2 : preview valeur -->
    <div class="text-[11.5px] text-muted-foreground mt-1.5 inline-flex items-center gap-1.5 min-w-0 w-full">
      {#if valuePreview}
        <span class="truncate min-w-0 flex-1" title={entry.value}>{valuePreview}</span>
      {:else}
        <span class="italic text-muted-foreground/60">vide</span>
      {/if}
    </div>

    <!-- Ligne 3 : timestamps + TTL -->
    <div class="text-[10.5px] text-muted-foreground/70 mt-1 inline-flex items-center gap-1.5">
      <span class="tabular-nums">il y a {relativeTime(entry.created_at)}</span>
      {#if entry.expires_at}
        <span>·</span>
        <span class="tabular-nums">{ttlBadge(entry.expires_at)}</span>
      {/if}
    </div>
  </div>

  <!-- Menu actions kebab (visible au hover) -->
  {#if hasActions}
    <div class="self-start mt-1" data-memory-row-actions>
      <ActionMenu
        align="end"
        class="p-1 min-w-[10rem]"
        triggerLabel="Actions"
        data-testid="memory-entry-row-menu-button-{entry.id}"
      >
        {#snippet triggerSlot(props)}
          <button
            type="button"
            {...props}
            class="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/60 transition-opacity hover:bg-muted/60 hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 {selected ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100'}"
            aria-label="Actions"
            data-testid="memory-entry-row-menu-button-{entry.id}"
          >
            <MoreHorizontal size={12} />
          </button>
        {/snippet}
        {#snippet body({ close })}
          {#if confirmingDelete}
            <div class="flex items-center gap-1 px-1 py-0.5">
              <Button variant="ghost" size="sm"
                type="button"
                onclick={() => { ondelete?.(); confirmingDelete = false; close(); }}
                class="inline-flex flex-1 items-center justify-center gap-1 rounded bg-destructive px-2 py-1 text-[11px] font-semibold text-white hover:bg-destructive/90"
              >
                <Check size={11} /> Confirmer
              </Button>
              <Button variant="ghost" size="sm"
                type="button"
                onclick={() => { confirmingDelete = false; close(); }}
                class="inline-flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-[11px] font-medium text-muted-foreground hover:bg-muted/60 hover:text-foreground"
              >
                <X size={11} /> Annuler
              </Button>
            </div>
          {:else}
            <ul class="flex flex-col gap-0.5" role="menu">
              {#if oncopy}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => { oncopy?.(); close(); }}
                    class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] text-foreground hover:bg-muted"
                  >
                    <Copy size={12} /> Copier la valeur
                  </button>
                </li>
              {/if}
              {#if onclick}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => { close(); onclick?.(); }}
                    class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] text-foreground hover:bg-muted"
                  >
                    <Eye size={12} /> Détails
                  </button>
                </li>
              {/if}
              {#if ondelete}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    onclick={() => { confirmingDelete = true; }}
                    class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] text-destructive hover:bg-destructive/10"
                  >
                    <Trash2 size={12} /> Supprimer
                  </button>
                </li>
              {/if}
            </ul>
          {/if}
        {/snippet}
      </ActionMenu>
    </div>
  {/if}
</div>
