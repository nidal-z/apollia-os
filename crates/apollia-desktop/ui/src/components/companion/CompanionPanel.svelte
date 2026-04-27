<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { Minus, X } from "lucide-svelte";
  import {
    companionStore,
    isCompanionPanelVisible,
    COMPANION_SESSION_TIMEOUT_MS,
    type CompanionPosition,
  } from "$lib/stores/companion";
  import { sidebarState } from "$lib/stores/layout";
  import { navigateTo, type Route } from "$lib/stores/navigation";
  import { addToast } from "$lib/components/ui/toast";
  import {
    clampPosition,
    edgeDistances,
    edgeGuides,
    snapToEdges,
    validateGeometry,
  } from "$lib/companion/snapGeometry";
  import ChatConversation from "../chat/ChatConversation.svelte";
  import CompanionDragHandle from "./CompanionDragHandle.svelte";
  import CompanionResizeHandle from "./CompanionResizeHandle.svelte";
  import CompanionSessionSkeleton from "./CompanionSessionSkeleton.svelte";
  import CompanionErrorState from "./CompanionErrorState.svelte";

  // Under `md`, the companion becomes full-screen so the chat remains usable
  // and the close button stays visible even at 375 px (/ E.40).
  const fullscreen = $derived($sidebarState === "drawer");

  const panelVisible = $derived(isCompanionPanelVisible($companionStore));
  const sessionId = $derived($companionStore.sessionId);
  const position = $derived($companionStore.position);
  const size = $derived($companionStore.size);
  const route = $derived($companionStore.currentRoute);
  const sessionStatus = $derived($companionStore.sessionStatus);
  const sessionError = $derived($companionStore.error);

  const showSkeleton = $derived(
    sessionStatus === "connecting" || sessionStatus === "creating",
  );

  // ── Session creation ──────────────────────────────────────────────────────

  let timeoutToastTimer: ReturnType<typeof setTimeout> | null = null;

  async function ensureSession() {
    if (sessionId || showSkeleton) return;
    if (timeoutToastTimer) clearTimeout(timeoutToastTimer);
    // Soft warning toast at the timeout boundary — the store also surfaces a
    // structured error state when the underlying IPC call eventually fails.
    timeoutToastTimer = setTimeout(() => {
      addToast($t("companion.session.timeout_toast_title"), "info");
    }, COMPANION_SESSION_TIMEOUT_MS);
    try {
      await companionStore.createSession(route);
    } catch {
      // Error state is rendered from the store; no rethrow needed.
    } finally {
      if (timeoutToastTimer) clearTimeout(timeoutToastTimer);
      timeoutToastTimer = null;
    }
  }

  $effect(() => {
    if (panelVisible && !sessionId && sessionStatus === "idle" && !sessionError) {
      untrack(() => void ensureSession());
    }
  });

  // ── Geometry initialisation ───────────────────────────────────────────────

  let computedPos = $state<CompanionPosition>({ x: 0, y: 0 });
  let computedSize = $state({ width: size.width, height: size.height });

  function viewport() {
    return { width: window.innerWidth, height: window.innerHeight };
  }

  function resolveInitialPosition(): CompanionPosition {
    if (position.x === -1 && position.y === -1) {
      return {
        x: window.innerWidth - size.width - 24,
        y: window.innerHeight - size.height - 24,
      };
    }
    return position;
  }

  onMount(() => {
    const validated = validateGeometry(
      { position: resolveInitialPosition(), size },
      viewport(),
    );
    computedPos = validated.position;
    computedSize = validated.size;
  });

  $effect(() => {
    // Keep local state in sync with the store (drag/resize commits update it).
    computedSize = { width: size.width, height: size.height };
  });

  $effect(() => {
    computedPos = resolveInitialPosition();
  });

  // ── Drag logic (pointer events + rAF throttle) ────────────────────────────

  let dragging = $state(false);
  let snapAnimating = $state(false);
  let dragOffset = { dx: 0, dy: 0 };
  let pendingPos: CompanionPosition | null = null;
  let rafHandle: number | null = null;

  function flushDrag() {
    rafHandle = null;
    if (!pendingPos) return;
    const clamped = clampPosition(pendingPos, computedSize, viewport());
    computedPos = clamped;
    pendingPos = null;
  }

  function onDragPointerDown(event: PointerEvent) {
    if (event.button !== 0 || fullscreen) return;
    event.preventDefault();
    dragging = true;
    snapAnimating = false;
    dragOffset = {
      dx: event.clientX - computedPos.x,
      dy: event.clientY - computedPos.y,
    };

    const target = event.target as HTMLElement;
    target.setPointerCapture?.(event.pointerId);

    function onMove(e: PointerEvent) {
      if (!dragging) return;
      pendingPos = {
        x: e.clientX - dragOffset.dx,
        y: e.clientY - dragOffset.dy,
      };
      if (rafHandle === null) rafHandle = requestAnimationFrame(flushDrag);
    }

    function onUp() {
      dragging = false;
      if (rafHandle !== null) {
        cancelAnimationFrame(rafHandle);
        rafHandle = null;
      }
      if (pendingPos) flushDrag();
      const { position: snapped, snapped: didSnap } = snapToEdges(
        computedPos,
        computedSize,
        viewport(),
      );
      if (didSnap) {
        snapAnimating = true;
        computedPos = snapped;
        setTimeout(() => (snapAnimating = false), 180);
      }
      companionStore.setPosition(computedPos);
      target.releasePointerCapture?.(event.pointerId);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  }

  // Snap-guides — recomputed only while dragging (cheap derivations).
  const guides = $derived.by(() => {
    if (!dragging) return { left: false, right: false, top: false, bottom: false };
    if (typeof window === "undefined") return { left: false, right: false, top: false, bottom: false };
    return edgeGuides(edgeDistances(computedPos, computedSize, viewport()));
  });

  // ── Resize logic ──────────────────────────────────────────────────────────

  let resizing = false;
  let resizeStart = { x: 0, y: 0, w: 0, h: 0 };
  let pendingSize: { width: number; height: number } | null = null;
  let resizeRaf: number | null = null;

  function flushResize() {
    resizeRaf = null;
    if (!pendingSize) return;
    computedSize = pendingSize;
    pendingSize = null;
  }

  function onResizePointerDown(event: PointerEvent) {
    if (event.button !== 0 || fullscreen) return;
    event.preventDefault();
    resizing = true;
    resizeStart = {
      x: event.clientX,
      y: event.clientY,
      w: computedSize.width,
      h: computedSize.height,
    };
    const target = event.target as HTMLElement;
    target.setPointerCapture?.(event.pointerId);

    function onMove(e: PointerEvent) {
      if (!resizing) return;
      const dw = e.clientX - resizeStart.x;
      const dh = e.clientY - resizeStart.y;
      pendingSize = {
        width: resizeStart.w + dw,
        height: resizeStart.h + dh,
      };
      if (resizeRaf === null) resizeRaf = requestAnimationFrame(flushResize);
    }

    function onUp() {
      resizing = false;
      if (resizeRaf !== null) {
        cancelAnimationFrame(resizeRaf);
        resizeRaf = null;
      }
      if (pendingSize) flushResize();
      companionStore.setSize(computedSize);
      target.releasePointerCapture?.(event.pointerId);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  }

  // ── Keyboard nudge (Cmd/Alt + arrows for 20 px) ───────────────────────────

  function onPanelKeyDown(event: KeyboardEvent) {
    if (fullscreen) return;
    if (!event.metaKey || !event.altKey) return;
    const STEP = 20;
    let dx = 0;
    let dy = 0;
    switch (event.key) {
      case "ArrowLeft":
        dx = -STEP;
        break;
      case "ArrowRight":
        dx = STEP;
        break;
      case "ArrowUp":
        dy = -STEP;
        break;
      case "ArrowDown":
        dy = STEP;
        break;
      default:
        return;
    }
    event.preventDefault();
    const next = clampPosition(
      { x: computedPos.x + dx, y: computedPos.y + dy },
      computedSize,
      viewport(),
    );
    const { position: snapped } = snapToEdges(next, computedSize, viewport());
    computedPos = snapped;
    companionStore.setPosition(snapped);
  }

  // ── Actions ───────────────────────────────────────────────────────────────

  function handleMinimize() {
    companionStore.minimizeCompanion();
  }

  function handleClose() {
    companionStore.closeCompanion();
  }

  function handleChatClose() {
    companionStore.closeCompanion();
  }

  function handleRetry() {
    companionStore.clearError();
    void ensureSession();
  }

  function handleErrorClose() {
    companionStore.clearError();
    companionStore.closeCompanion();
  }

  function handleOpenSettings() {
    if (!sessionError) {
      navigateTo("settings");
      return;
    }
    const map: Record<string, Route> = {
      network: "integrations",
      permissions: "settings-permission-rules",
      timeout: "llm",
      internal: "settings",
    };
    navigateTo(map[sessionError.kind] ?? "settings");
  }
</script>

{#if panelVisible}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="complementary"
    aria-label={$t("companion.aria_label")}
    class="fixed flex flex-col overflow-hidden border border-border bg-background shadow-2xl focus:outline-none"
    class:companion-fullscreen={fullscreen}
    class:rounded-xl={!fullscreen}
    class:companion-snap={snapAnimating}
    style:z-index="65"
    style:left={fullscreen ? undefined : `${computedPos.x}px`}
    style:top={fullscreen ? undefined : `${computedPos.y}px`}
    style:width={fullscreen ? undefined : `${computedSize.width}px`}
    style:height={fullscreen ? undefined : `${computedSize.height}px`}
    tabindex="-1"
    onkeydown={onPanelKeyDown}
    data-testid="companion-panel"
    data-fullscreen={fullscreen ? "true" : "false"}
    data-dragging={dragging ? "true" : "false"}
  >
    <!-- Title bar (drag zone) -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex shrink-0 items-center gap-2 border-b border-border bg-muted/50 px-3 py-2 select-none"
      class:cursor-grab={!fullscreen}
      class:active:cursor-grabbing={!fullscreen}
      onpointerdown={fullscreen ? undefined : onDragPointerDown}
    >
      {#if !fullscreen}
        <CompanionDragHandle />
      {/if}
      <img src="/logo.svg" alt="Apollia" class="h-4 w-4 shrink-0" />
      <span class="flex-1 truncate text-sm font-medium text-foreground">
        {$t("companion.title")}
      </span>
      <button
        class="inline-flex items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground {fullscreen
          ? 'h-11 w-11'
          : 'h-6 w-6'}"
        onpointerdown={(e) => e.stopPropagation()}
        onclick={handleMinimize}
        aria-label={$t("companion.minimize")}
        data-testid="companion-minimize"
      >
        <Minus size={fullscreen ? 18 : 14} />
      </button>
      <button
        class="inline-flex items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground {fullscreen
          ? 'h-11 w-11'
          : 'h-6 w-6'}"
        onpointerdown={(e) => e.stopPropagation()}
        onclick={handleClose}
        aria-label={$t("companion.close")}
        data-testid="companion-close"
      >
        <X size={fullscreen ? 18 : 14} />
      </button>
    </div>

    <!-- Chat content / skeleton / error -->
    <div class="min-h-0 flex-1 overflow-hidden">
      {#if sessionError}
        <CompanionErrorState
          error={sessionError}
          onRetry={handleRetry}
          onOpenSettings={handleOpenSettings}
          onClose={handleErrorClose}
        />
      {:else if showSkeleton}
        <CompanionSessionSkeleton status={sessionStatus} />
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

    {#if !fullscreen}
      <CompanionResizeHandle onpointerdown={onResizePointerDown} />
    {/if}
  </div>

  <!-- Snap-to-edge guides (only while dragging, viewport-fixed dashed lines) -->
  {#if !fullscreen && dragging}
    {#if guides.left}
      <div
        class="pointer-events-none fixed left-0 top-0 bottom-0 z-[64] border-l border-dashed border-primary/40"
        data-testid="companion-snap-guide-left"
      ></div>
    {/if}
    {#if guides.right}
      <div
        class="pointer-events-none fixed right-0 top-0 bottom-0 z-[64] border-r border-dashed border-primary/40"
        data-testid="companion-snap-guide-right"
      ></div>
    {/if}
    {#if guides.top}
      <div
        class="pointer-events-none fixed left-0 right-0 top-0 z-[64] border-t border-dashed border-primary/40"
        data-testid="companion-snap-guide-top"
      ></div>
    {/if}
    {#if guides.bottom}
      <div
        class="pointer-events-none fixed left-0 right-0 bottom-0 z-[64] border-b border-dashed border-primary/40"
        data-testid="companion-snap-guide-bottom"
      ></div>
    {/if}
  {/if}
{/if}

<style>
  .companion-snap {
    transition:
      left 150ms cubic-bezier(0.34, 1.56, 0.64, 1),
      top 150ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  /* Fullscreen uses `dvh` (dynamic viewport) so mobile URL-bar toggling
     doesn't cut off the close button. Falls back to `100vh` on browsers
     without dvh support. */
  .companion-fullscreen {
    inset: 0;
    height: 100vh;
    height: 100dvh;
    width: 100vw;
    width: 100dvw;
  }
</style>
