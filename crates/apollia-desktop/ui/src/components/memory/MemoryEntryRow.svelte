<script lang="ts">
  import { Clock, Database, Cog, MoreHorizontal, Trash2, Eye, Copy, Check, X, Sparkles } from "lucide-svelte";
  import { t, locale } from "svelte-i18n";
  import type { MemoryEntry } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
  import { Button } from "$lib/components/ui/button";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";

  interface Props {
    entry: MemoryEntry;
    /** True quand cette row est sélectionnée (panneau détail ouvert sur elle). */
    selected?: boolean;
    /** True en mode résultat de recherche : affiche le score BM25 + le relevance-meter. */
    searching?: boolean;
    /** Relevance normalisée 0..1 pour la barre (reprend l'idiome chat `.tb-mbar`). */
    relevance?: number;
    onclick?: () => void;
    oncopy?: () => void;
    ondelete?: () => void;
  }

  let { entry, selected = false, searching = false, relevance, onclick, oncopy, ondelete }: Props = $props();

  const typeLabel = $derived(
    entry.entry_type === "episodic"
      ? $t("memory.type.episodic")
      : entry.entry_type === "semantic"
        ? $t("memory.type.semantic")
        : entry.entry_type === "procedural"
          ? $t("memory.type.procedural")
          : entry.entry_type,
  );
  const typeAbbr = $derived(typeLabel.slice(0, 4).toLowerCase());

  // ── Preview tronquée (le détail complet vit dans le Sheet) ───────────────────
  const valuePreview = $derived.by(() => {
    if (!entry.value) return "";
    let v = entry.value.trim();
    if (v.startsWith('"') && v.endsWith('"')) v = v.slice(1, -1);
    return v.length > 120 ? v.slice(0, 120) + "…" : v;
  });

  // ── Temps relatif localisé (Intl, aucune chaîne codée en dur) ────────────────
  function relative(iso: string): string {
    const rtf = new Intl.RelativeTimeFormat($locale ?? "en", { numeric: "auto", style: "narrow" });
    const diff = new Date(iso).getTime() - Date.now();
    const abs = Math.abs(diff);
    const m = 60_000, h = 3_600_000, d = 86_400_000;
    if (abs < m) return rtf.format(Math.round(diff / 1000), "second");
    if (abs < h) return rtf.format(Math.round(diff / m), "minute");
    if (abs < d) return rtf.format(Math.round(diff / h), "hour");
    if (abs < 7 * d) return rtf.format(Math.round(diff / d), "day");
    if (abs < 30 * d) return rtf.format(Math.round(diff / (7 * d)), "week");
    if (abs < 365 * d) return rtf.format(Math.round(diff / (30 * d)), "month");
    return rtf.format(Math.round(diff / (365 * d)), "year");
  }
  const createdRel = $derived(relative(entry.created_at));
  const expiresRel = $derived(entry.expires_at ? relative(entry.expires_at) : "");
  const scoreLabel = $derived(
    searching && entry.score !== null && entry.score !== undefined ? entry.score.toFixed(2) : "",
  );
  const relPct = $derived(relevance != null ? Math.round(Math.max(0, Math.min(1, relevance)) * 100) : null);

  // ── Menu partagé (kebab + clic droit) ────────────────────────────────────────
  let menuOpen = $state(false);
  let confirmingDelete = $state(false);
  $effect(() => {
    if (!menuOpen) confirmingDelete = false;
  });
  function requestDelete(): void {
    menuOpen = true;
    confirmingDelete = true;
  }

  const menuItems = $derived<ActionMenuItem[]>([
    ...(onclick ? [{ id: "open", label: $t("memory.action_open"), icon: Eye, onclick: () => onclick?.() }] : []),
    ...(oncopy ? [{ id: "copy", label: $t("memory.copy_label"), icon: Copy, onclick: () => oncopy?.() }] : []),
    ...(ondelete
      ? [{ id: "delete", label: $t("memory.delete_label"), icon: Trash2, variant: "destructive" as const, onclick: requestDelete }]
      : []),
  ]);
  const hasActions = $derived(menuItems.length > 0);

  function onRowClick(ev: MouseEvent): void {
    if ((ev.target as HTMLElement | null)?.closest("[data-memory-row-actions]")) return;
    onclick?.();
  }
  function onRowKeydown(ev: KeyboardEvent): void {
    if (ev.key !== "Enter" && ev.key !== " ") return;
    if ((ev.target as HTMLElement | null)?.closest("[data-memory-row-actions]")) return;
    ev.preventDefault();
    onclick?.();
  }
</script>

{#snippet rowContent()}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    role={onclick ? "button" : undefined}
    tabindex={onclick ? 0 : undefined}
    onclick={onclick ? onRowClick : undefined}
    onkeydown={onclick ? onRowKeydown : undefined}
    class="group relative flex cursor-pointer items-start gap-3 border-b border-border/40 px-6 py-3.5 transition-colors {selected
      ? 'bg-primary/10'
      : 'bg-transparent hover:bg-muted/40'}"
    data-testid="memory-entry-row-{entry.id}"
  >
    {#if selected}
      <span class="absolute inset-y-0 left-0 w-0.5 bg-gradient-primary" aria-hidden="true"></span>
    {/if}
    <div
      class="mt-0.5 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md {selected
        ? 'bg-gradient-primary text-primary-foreground'
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

    <div class="min-w-0 flex-1">
      <!-- Ligne 1 : key + badges -->
      <div class="flex min-w-0 items-center gap-2">
        <code class="truncate font-mono text-body-sm text-foreground" style:font-weight={selected ? 600 : 500} title={entry.key}>
          {entry.key}
        </code>
        <span class="inline-flex shrink-0 items-center rounded border border-border/60 bg-background px-1.5 py-px text-overline font-medium tracking-tight text-muted-foreground">
          {typeAbbr}
        </span>
        {#if scoreLabel}
          <span class="inline-flex shrink-0 items-center rounded bg-info/10 px-1.5 py-px font-mono text-caption tabular-nums text-info" title={$t("memory.meta_score")}>
            {scoreLabel}
          </span>
        {/if}
      </div>

      {#if $uiMode === "operator"}
        <!-- Aperçu humain + temps relatif -->
        <div class="mt-1.5 flex w-full min-w-0 items-center gap-1.5 text-caption text-muted-foreground">
          {#if valuePreview}
            <span class="min-w-0 flex-1 truncate" title={entry.value}>{valuePreview}</span>
          {:else}
            <span class="italic text-muted-foreground/60">{$t("memory.value_empty")}</span>
          {/if}
        </div>
        <div class="mt-1 inline-flex items-center gap-1.5 text-overline font-normal tracking-normal tabular-nums text-muted-foreground/70">
          <span>{createdRel}</span>
          {#if expiresRel}
            <span aria-hidden="true">·</span>
            <!-- i18n-ignore: time-to-live acronym, identical in every locale -->
            <span class="text-warning">TTL {expiresRel}</span>
          {/if}
        </div>
      {:else}
        <!-- Fiche brute technique -->
        <dl class="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 font-mono text-caption">
          <dt class="text-muted-foreground/75">id</dt>
          <dd class="break-all tabular-nums text-foreground/85">{entry.id}</dd>
          <dt class="text-muted-foreground/75">type</dt>
          <dd class="text-success">"{entry.entry_type}"</dd>
          <dt class="text-muted-foreground/75">score</dt>
          <dd class="text-warning tabular-nums">{entry.score ?? "null"}</dd>
          <dt class="text-muted-foreground/75">created_at</dt>
          <dd class="text-success">"{entry.created_at}"</dd>
          <dt class="text-muted-foreground/75">expires_at</dt>
          <dd class={entry.expires_at ? "text-success" : "text-warning"}>{entry.expires_at ? `"${entry.expires_at}"` : "null"}</dd>
        </dl>
        <div class="mt-1.5 inline-flex items-center gap-1 text-overline font-normal italic tracking-normal text-muted-foreground/80">
          <Sparkles size={11} class="text-primary" />
          {entry.entry_type === "episodic" ? $t("memory.embeddings.none") : $t("memory.embeddings.vector")}
        </div>
      {/if}

      {#if searching && relPct !== null}
        <div class="tb-mbar" aria-hidden="true"><span style="width: {relPct}%"></span></div>
      {/if}
    </div>

    {#if hasActions}
      <div class="mt-1 self-start" data-memory-row-actions>
        <ActionMenu bind:open={menuOpen} align="end" class="min-w-[10rem] p-1" triggerLabel={$t("memory.entry_actions")} data-testid="memory-entry-row-menu-button-{entry.id}">
          {#snippet triggerSlot(props)}
            <button
              type="button"
              {...props}
              class="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground/60 transition-opacity hover:bg-muted/60 hover:text-foreground focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 {selected
                ? 'opacity-100'
                : 'opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100'}"
              aria-label={$t("memory.entry_actions")}
              data-testid="memory-entry-row-menu-button-{entry.id}"
            >
              <MoreHorizontal size={12} />
            </button>
          {/snippet}
          {#snippet body({ close })}
            {#if confirmingDelete}
              <div class="flex items-center gap-1 px-1 py-0.5">
                <Button variant="ghost" size="sm" type="button" onclick={() => { ondelete?.(); confirmingDelete = false; close(); }} class="inline-flex flex-1 items-center justify-center gap-1 rounded bg-destructive px-2 py-1 text-caption font-semibold text-primary-foreground hover:bg-destructive/90">
                  <Check size={11} /> {$t("memory.confirm")}
                </Button>
                <Button variant="ghost" size="sm" type="button" onclick={() => { confirmingDelete = false; close(); }} class="inline-flex flex-1 items-center justify-center gap-1 rounded px-2 py-1 text-caption font-medium text-muted-foreground hover:bg-muted/60 hover:text-foreground">
                  <X size={11} /> {$t("memory.cancel")}
                </Button>
              </div>
            {:else}
              <ul class="flex flex-col gap-0.5" role="menu">
                {#each menuItems as it (it.id)}
                  {@const ItemIcon = it.icon}
                  <li role="none">
                    <button
                      type="button"
                      role="menuitem"
                      onclick={() => { if (it.id === "delete") { it.onclick(); } else { close(); it.onclick(); } }}
                      class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs {it.variant === 'destructive' ? 'text-destructive hover:bg-destructive/10' : 'text-foreground hover:bg-muted'}"
                    >
                      {#if ItemIcon}<ItemIcon size={12} />{/if}
                      {it.label}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {/snippet}
        </ActionMenu>
      </div>
    {/if}
  </div>
{/snippet}

{#if hasActions}
  <ContextMenu items={menuItems} data-testid="memory-entry-row-ctx-{entry.id}">
    {@render rowContent()}
  </ContextMenu>
{:else}
  {@render rowContent()}
{/if}
