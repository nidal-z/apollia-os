<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Bot, Minus, X } from "lucide-svelte";
  import {
    companionStore,
    isCompanionPanelVisible,
    isCompanionRestoreVisible,
    type CompanionPosition,
  } from "$lib/stores/companion";
  import ChatConversation from "../chat/ChatConversation.svelte";

  // ── Panel state ────────────────────────────────────────────────────────────

  const panelVisible = $derived(isCompanionPanelVisible($companionStore));
  const restoreVisible = $derived(isCompanionRestoreVisible($companionStore));
  const sessionId = $derived($companionStore.sessionId);
  const position = $derived($companionStore.position);
  const size = $derived($companionStore.size);
  const route = $derived($companionStore.currentRoute);

  let isCreatingSession = $state(false);
  let sessionError = $state<string | null>(null);

  // ── Session creation ───────────────────────────────────────────────────────

  async function ensureSession() {
    if (sessionId) return;
    if (isCreatingSession) return;
    isCreatingSession = true;
    sessionError = null;
    try {
      await companionStore.createSession(route);
    } catch (err) {
      sessionError = err instanceof Error ? err.message : String(err);
    } finally {
      isCreatingSession = false;
    }
  }

  $effect(() => {
    if (panelVisible && !sessionId) {
      void ensureSession();
    }
  });

  // ── Drag logic ─────────────────────────────────────────────────────────────

  let dragging = $state(false);
  let dragOffset = $state({ dx: 0, dy: 0 });

  function resolveInitialPosition(): CompanionPosition {
    const pos = position;
    if (pos.x === -1 && pos.y === -1) {
      return {
        x: window.innerWidth - size.width - 24,
        y: window.innerHeight - size.height - 24,
      };
    }
    return pos;
  }

  let computedPos = $state<CompanionPosition>({ x: 0, y: 0 });

  onMount(() => {
    computedPos = resolveInitialPosition();
  });

  $effect(() => {
    // Re-resolve when the stored position changes (e.g. on load or reset).
    computedPos = resolveInitialPosition();
  });

  function onTitleMouseDown(event: MouseEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    dragging = true;
    dragOffset = {
      dx: event.clientX - computedPos.x,
      dy: event.clientY - computedPos.y,
    };

    function onMouseMove(e: MouseEvent) {
      if (!dragging) return;
      const newX = e.clientX - dragOffset.dx;
      const newY = e.clientY - dragOffset.dy;
      const clamped = {
        x: Math.max(0, Math.min(newX, window.innerWidth - size.width)),
        y: Math.max(0, Math.min(newY, window.innerHeight - size.height)),
      };
      computedPos = clamped;
    }

    function onMouseUp() {
      dragging = false;
      companionStore.setPosition(computedPos);
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    }

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  }

  // ── Resize logic ───────────────────────────────────────────────────────────

  let resizing = $state(false);
  let resizeStart = $state({ x: 0, y: 0, w: 0, h: 0 });

  function onResizeMouseDown(event: MouseEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    resizing = true;
    resizeStart = {
      x: event.clientX,
      y: event.clientY,
      w: size.width,
      h: size.height,
    };

    function onMouseMove(e: MouseEvent) {
      if (!resizing) return;
      const dw = e.clientX - resizeStart.x;
      const dh = e.clientY - resizeStart.y;
      companionStore.setSize({
        width: resizeStart.w + dw,
        height: resizeStart.h + dh,
      });
    }

    function onMouseUp() {
      resizing = false;
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    }

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  function handleMinimize() {
    companionStore.minimizeCompanion();
  }

  function handleClose() {
    companionStore.closeCompanion();
  }

  function handleChatClose() {
    // Closing from within ChatConversation collapses the companion instead of
    // destroying the session.
    companionStore.closeCompanion();
  }
</script>

{#if restoreVisible}
  <button
    class="fixed z-[65] flex h-12 w-12 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg transition-transform hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    style:bottom="24px"
    style:right="24px"
    onclick={() => companionStore.restoreCompanion()}
    aria-label={$t("companion.restore")}
    data-testid="companion-restore"
  >
    <Bot size={22} />
  </button>
{/if}

{#if panelVisible}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="complementary"
    aria-label={$t("companion.aria_label")}
    class="fixed flex flex-col overflow-hidden rounded-xl border border-border bg-background shadow-2xl"
    style:z-index="65"
    style:left="{computedPos.x}px"
    style:top="{computedPos.y}px"
    style:width="{size.width}px"
    style:height="{size.height}px"
    data-testid="companion-panel"
  >
    <!-- Title bar (drag handle) -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex shrink-0 cursor-grab items-center gap-2 border-b border-border bg-muted/50 px-3 py-2 select-none active:cursor-grabbing"
      onmousedown={onTitleMouseDown}
    >
      <Bot size={16} class="shrink-0 text-primary" />
      <span class="flex-1 truncate text-sm font-medium text-foreground">
        {$t("companion.title")}
      </span>
      <button
        class="flex h-6 w-6 items-center justify-center rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        onclick={handleMinimize}
        aria-label={$t("companion.minimize")}
        data-testid="companion-minimize"
      >
        <Minus size={14} />
      </button>
      <button
        class="flex h-6 w-6 items-center justify-center rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
        onclick={handleClose}
        aria-label={$t("companion.close")}
        data-testid="companion-close"
      >
        <X size={14} />
      </button>
    </div>

    <!-- Chat content -->
    <div class="min-h-0 flex-1 overflow-hidden">
      {#if isCreatingSession}
        <div class="flex h-full items-center justify-center text-muted-foreground">
          <span class="text-sm">{$t("companion.starting")}</span>
        </div>
      {:else if sessionError}
        <div class="flex h-full flex-col items-center justify-center gap-3 p-4 text-center">
          <p class="text-sm text-destructive">{sessionError}</p>
          <button
            class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 transition-colors"
            onclick={() => void ensureSession()}
          >
            {$t("companion.retry")}
          </button>
        </div>
      {:else if sessionId}
        <ChatConversation
          {sessionId}
          onclose={handleChatClose}
          embedded={true}
          hideConfig={true}
        />
      {:else}
        <div class="flex h-full items-center justify-center text-muted-foreground">
          <span class="text-sm">{$t("companion.no_session")}</span>
        </div>
      {/if}
    </div>

    <!-- Resize handle (bottom-right corner) -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute bottom-0 right-0 h-4 w-4 cursor-se-resize opacity-30 hover:opacity-70 transition-opacity"
      onmousedown={onResizeMouseDown}
      aria-hidden="true"
    >
      <svg viewBox="0 0 16 16" class="h-full w-full fill-foreground">
        <path d="M11 11L15 15M7 11L15 15M11 7L15 15" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" fill="none" />
      </svg>
    </div>
  </div>
{/if}
