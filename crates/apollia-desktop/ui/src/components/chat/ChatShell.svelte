<script lang="ts">
  /**
   * 3-column chat shell (US-SP42-022).
   *
   * Slots:
   *   - `sessions`: list of chat sessions (rail @ md+, drawer overlay @ sm)
   *   - `conversation`: active conversation (fills remaining space, min-w-0)
   *   - `context`: ContextDrawer content (inline rail @ lg+, drawer overlay <lg)
   *
   * Width contract:
   *   - Sessions rail: clamp(260px, 22vw, 320px) — tokens.chatLayout.sessions*
   *   - ContextDrawer inline: user-resizable between 320 and 420 px, persisted
   *     via `chatLayout` store (`chat.contextDrawer.width`).
   *
   * Breakpoints: see `$lib/stores/chatLayout` for the full responsive contract.
   * `prefers-reduced-motion` disables the enter/exit transitions on overlays.
   */
  import type { Snippet } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { chatLayout } from "$lib/design/tokens";
  import {
    contextDrawerMode,
    contextDrawerOpen,
    contextDrawerWidth,
    sessionsPaneMode,
    sessionsDrawerOpen,
    sessionsSidebarCollapsed,
    closeSessionsDrawer,
    setContextDrawerWidth,
  } from "$lib/stores/chatLayout";

  interface Props {
    sessions: Snippet;
    conversation: Snippet;
    context: Snippet;
  }
  let { sessions, conversation, context }: Props = $props();

  const reducedMotion = typeof window !== "undefined"
    && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  // Drag-resize state for the ContextDrawer inline rail (lg+ only).
  let dragStartX = 0;
  let dragStartWidth = 0;
  let dragging = $state(false);

  function onResizePointerDown(ev: PointerEvent) {
    dragStartX = ev.clientX;
    dragStartWidth = $contextDrawerWidth;
    dragging = true;
    (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId);
    window.addEventListener("pointermove", onResizePointerMove);
    window.addEventListener("pointerup", onResizePointerUp, { once: true });
  }
  function onResizePointerMove(ev: PointerEvent) {
    // Handle is on the LEFT edge of the context rail — dragging left widens it.
    const delta = dragStartX - ev.clientX;
    setContextDrawerWidth(dragStartWidth + delta);
  }
  function onResizePointerUp() {
    dragging = false;
    window.removeEventListener("pointermove", onResizePointerMove);
  }

  function onResizeKeydown(ev: KeyboardEvent) {
    const step = ev.shiftKey ? 32 : 8;
    if (ev.key === "ArrowLeft") { ev.preventDefault(); setContextDrawerWidth($contextDrawerWidth + step); }
    else if (ev.key === "ArrowRight") { ev.preventDefault(); setContextDrawerWidth($contextDrawerWidth - step); }
  }

  // Inline context width resolved from store, clamped defensively.
  const inlineContextWidth = $derived(
    Math.min(chatLayout.contextMaxPx, Math.max(chatLayout.contextMinPx, $contextDrawerWidth))
  );

  function closeContext() { contextDrawerOpen.set(false); }
</script>

<div class="flex h-full min-h-0 w-full" data-testid="chat-shell">
  <!-- Sessions rail — inline @ md+ (sticky @ md, full column @ lg+).
       Collapsed via Cmd+B (US-SP42-033) on `lg` only — collapsing on
       narrower viewports is meaningless because the rail is sticky/drawer. -->
  {#if $sessionsPaneMode !== "drawer" && !($sessionsPaneMode === "column" && $sessionsSidebarCollapsed)}
    <aside
      class="shrink-0 flex flex-col border-r border-border/30 bg-background/40 min-h-0"
      style="width: clamp({chatLayout.sessionsMinPx}px, 22vw, {chatLayout.sessionsMaxPx}px);"
      data-testid="chat-shell-sessions"
      data-pane="sessions"
      data-mode={$sessionsPaneMode}
    >
      {@render sessions()}
    </aside>
  {/if}

  <!-- Conversation column — always present, elastic width, own vertical scroll. -->
  <section
    class="flex min-w-0 flex-1 flex-col min-h-0"
    data-testid="chat-shell-conversation"
  >
    {@render conversation()}
  </section>

  <!-- ContextDrawer inline (lg+ only, when toggled open). -->
  {#if $contextDrawerMode === "inline" && $contextDrawerOpen}
    <aside
      class="relative shrink-0 flex flex-col border-l border-border/30 bg-background/60 min-h-0"
      style="width: {inlineContextWidth}px;"
      data-testid="chat-shell-context"
      data-pane="context"
    >
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize context drawer"
        aria-valuemin={chatLayout.contextMinPx}
        aria-valuemax={chatLayout.contextMaxPx}
        aria-valuenow={inlineContextWidth}
        tabindex="0"
        class="absolute left-0 top-0 z-10 h-full w-1 cursor-col-resize touch-none select-none
               transition-colors hover:bg-primary/40 focus-visible:bg-primary/50 focus-visible:outline-none
               {dragging ? 'bg-primary/60' : ''}"
        onpointerdown={onResizePointerDown}
        onkeydown={onResizeKeydown}
        data-testid="chat-shell-context-resize"
      ></div>
      {@render context()}
    </aside>
  {/if}
</div>

<!-- Sessions overlay drawer (sm only). -->
{#if $sessionsPaneMode === "drawer" && $sessionsDrawerOpen}
  <div
    class="fixed inset-0 backdrop-warm"
    style="z-index: var(--z-backdrop);"
    role="button"
    tabindex="-1"
    onclick={closeSessionsDrawer}
    onkeydown={(e) => e.key === 'Escape' && closeSessionsDrawer()}
    transition:fade={{ duration: reducedMotion ? 0 : 200 }}
  ></div>
  <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
  <aside
    class="fixed inset-y-0 left-0 flex flex-col border-r border-border glass-panel"
    style="z-index: var(--z-overlay); width: min({chatLayout.sessionsMaxPx}px, 85vw);"
    role="dialog"
    aria-modal="true"
    transition:fly={{ x: -chatLayout.sessionsMaxPx, duration: reducedMotion ? 0 : 260 }}
    data-testid="chat-shell-sessions-drawer"
  >
    {@render sessions()}
  </aside>
{/if}

<!-- Context overlay drawer (sm/md). -->
{#if $contextDrawerMode === "overlay" && $contextDrawerOpen}
  <div
    class="fixed inset-0 backdrop-warm"
    style="z-index: var(--z-backdrop);"
    role="button"
    tabindex="-1"
    onclick={closeContext}
    onkeydown={(e) => e.key === 'Escape' && closeContext()}
    transition:fade={{ duration: reducedMotion ? 0 : 200 }}
  ></div>
  <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
  <aside
    class="fixed inset-y-0 right-0 flex flex-col border-l border-border glass-panel"
    style="z-index: var(--z-overlay); width: min({chatLayout.contextMaxPx}px, 90vw);"
    role="dialog"
    aria-modal="true"
    transition:fly={{ x: chatLayout.contextMaxPx, duration: reducedMotion ? 0 : 260 }}
    data-testid="chat-shell-context-drawer"
  >
    {@render context()}
  </aside>
{/if}
