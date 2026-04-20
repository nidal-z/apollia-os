<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import {
    activeChatSessions,
    closedChatSessions,
    pendingChatSessionId,
    openNewChatRequested,
  } from "$lib/stores/chat";
  import { chatSessions } from "$lib/stores/sse";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { ChatSessionSummary } from "$lib/types";
  import { tourOpenChatPicker } from "$lib/stores/tour";
  import ChatConversation from "../components/chat/ChatConversation.svelte";
  import ChatSessionCard from "../components/chat/ChatSessionCard.svelte";
  import ChatSessionsSidebar from "../components/chat/ChatSessionsSidebar.svelte";
  import ChatShell from "../components/chat/ChatShell.svelte";
  import ContextDrawer from "../components/chat/ContextDrawer.svelte";
  import EmptySessionsState from "../components/chat/EmptySessionsState.svelte";
  import QuickPicker from "../components/chat/QuickPicker.svelte";
  import RuntimeDisconnectedBanner from "../components/chat/RuntimeDisconnectedBanner.svelte";
  import { startRuntimeHealthMonitor } from "$lib/stores/runtimeHealth";
  import {
    decoratedSessions,
    markSessionRead,
  } from "$lib/stores/chatSessions";
  import {
    contextDrawerOpen,
    contextDrawerActiveTab,
    toggleContextDrawer,
    toggleSessionsSidebar,
    openSessionsDrawer,
    closeSessionsDrawer,
  } from "$lib/stores/chatLayout";
  import { currentSession } from "$lib/stores/chat";
  import {
    installChatShortcuts,
    describeBinding,
    type ShortcutBinding,
    MOD_LABEL,
  } from "$lib/shortcuts/chatShortcuts";
  import { focusMessage } from "$lib/shortcuts/messageNav";
  import { commandPaletteOpen } from "$lib/stores/commandPalette";
  import { registerShortcuts } from "$lib/stores/shortcuts";
  import CommandPalette from "../components/chat/CommandPalette.svelte";
  import ShortcutsHelpDialog from "../components/chat/ShortcutsHelpDialog.svelte";

  let selectedSessionId = $state<string | null>(null);
  let showNewChatPicker = $state(false);
  let showShortcutsHelp = $state(false);

  // ── Shortcut registry (US-SP42-033, B.29) ──────────────────────────────
  // One central place declares every chat-route hotkey. Bindings are keyed
  // on `event.code` so AZERTY/QWERTY layouts share the same physical keys.
  const SHORTCUT_BINDINGS: ShortcutBinding[] = [
    {
      id: "chat.new",
      chord: { code: "KeyN", modifiers: ["mod"] },
      group: "chat.shortcut_group.chat",
      descriptionKey: "chat.shortcut.new_chat",
      keys: [MOD_LABEL, "N"],
      run: () => openNewChatPicker(),
    },
    {
      id: "chat.send",
      chord: { code: "Enter", modifiers: ["mod"] },
      group: "chat.shortcut_group.chat",
      descriptionKey: "chat.shortcut.send",
      keys: [MOD_LABEL, "Enter"],
      // Cmd+Enter is consumed by ChatInput when the textarea is focused;
      // this binding is a no-op fallback so it shows up in the Help dialog.
      allowInTextarea: true,
      run: () => false,
    },
    {
      id: "chat.palette",
      chord: { code: "KeyK", modifiers: ["mod"] },
      group: "chat.shortcut_group.navigation",
      descriptionKey: "chat.shortcut.palette",
      keys: [MOD_LABEL, "K"],
      allowInTextarea: true,
      run: () => commandPaletteOpen.update((v) => !v),
    },
    {
      id: "chat.search",
      chord: { code: "KeyF", modifiers: ["mod"] },
      group: "chat.shortcut_group.navigation",
      descriptionKey: "chat.shortcut.search_sidebar",
      keys: [MOD_LABEL, "F"],
      run: () => {
        document
          .querySelector<HTMLInputElement>("[data-testid='session-search-input']")
          ?.focus();
      },
    },
    {
      id: "chat.toggle_sidebar",
      chord: { code: "KeyB", modifiers: ["mod"] },
      group: "chat.shortcut_group.panels",
      descriptionKey: "chat.shortcut.toggle_sidebar",
      keys: [MOD_LABEL, "B"],
      run: () => toggleSessionsSidebar(),
    },
    {
      id: "chat.toggle_context",
      chord: { code: "KeyD", modifiers: ["mod", "shift"] },
      group: "chat.shortcut_group.panels",
      descriptionKey: "chat.shortcut.toggle_context",
      keys: [MOD_LABEL, "Shift", "D"],
      run: () => toggleContextDrawer(),
    },
    {
      id: "chat.toggle_artifacts",
      chord: { code: "KeyA", modifiers: ["mod", "shift"] },
      group: "chat.shortcut_group.panels",
      descriptionKey: "chat.shortcut.toggle_artifacts",
      keys: [MOD_LABEL, "Shift", "A"],
      run: () => {
        contextDrawerActiveTab.set("artifacts");
        contextDrawerOpen.update((o) => !o);
      },
    },
    {
      id: "chat.help",
      chord: { code: "Slash", modifiers: ["mod"] },
      group: "chat.shortcut_group.navigation",
      descriptionKey: "chat.shortcut.help",
      keys: [MOD_LABEL, "/"],
      allowInTextarea: true,
      run: () => (showShortcutsHelp = !showShortcutsHelp),
    },
    {
      id: "chat.next_message",
      chord: { code: "KeyJ", forbid: ["mod", "shift", "alt"] },
      group: "chat.shortcut_group.messages",
      descriptionKey: "chat.shortcut.next_message",
      keys: ["J"],
      run: () => focusMessage(1),
    },
    {
      id: "chat.prev_message",
      chord: { code: "KeyK", forbid: ["mod", "shift", "alt"] },
      group: "chat.shortcut_group.messages",
      descriptionKey: "chat.shortcut.prev_message",
      keys: ["K"],
      run: () => focusMessage(-1),
    },
    {
      id: "chat.stop_or_close",
      chord: { code: "Escape" },
      group: "chat.shortcut_group.chat",
      descriptionKey: "chat.shortcut.stop_or_close",
      keys: ["Esc"],
      run: () => {
        if ($commandPaletteOpen) {
          commandPaletteOpen.set(false);
          return;
        }
        if (showShortcutsHelp) {
          showShortcutsHelp = false;
          return;
        }
        // Streaming abort + per-modal Esc are handled by the owning
        // components — fall through so they keep working.
        return false;
      },
    },
  ];

  onMount(() => {
    const unsub = pendingChatSessionId.subscribe((id) => {
      if (id) { selectedSessionId = id; pendingChatSessionId.set(null); }
    });
    const unsubTour = tourOpenChatPicker.subscribe((open) => {
      if (open) { openNewChatPicker(); tourOpenChatPicker.set(false); }
    });
    const uninstall = installChatShortcuts(SHORTCUT_BINDINGS);
    const unregisterHelp = registerShortcuts(SHORTCUT_BINDINGS.map(describeBinding));
    const stopHealthMonitor = startRuntimeHealthMonitor();
    return () => {
      unsub();
      unsubTour();
      uninstall();
      unregisterHelp();
      stopHealthMonitor();
    };
  });

  // The preset payload is currently informational — QuickPicker has no
  // preset prop yet (see story US-SP42-024 follow-up). We accept and
  // ignore it so the palette wiring stays forward-compatible.
  function openNewChatPicker(_preset?: { templateId?: string; agentName?: string }) {
    showNewChatPicker = true;
  }

  // External triggers (sidebar button, command palette) push Date.now() into
  // this store; we open the picker on every change. Initialise lastSignal to
  // the current value so mounting the route doesn't immediately open it.
  let lastNewChatSignal = $state(0);
  $effect(() => {
    const v = $openNewChatRequested;
    if (v && v !== lastNewChatSignal) {
      lastNewChatSignal = v;
      openNewChatPicker();
    }
  });
  function closeNewChatPicker() {
    showNewChatPicker = false;
  }

  function openSessionFromPalette(id: string): void {
    selectedSessionId = id;
    markSessionRead(id);
  }

  function appendSlashCommand(cmdId: string): void {
    // Surface the slash token in the active textarea so it lands in the
    // existing command pipeline (`SlashCommandMenu` reuses `chatInputAppend`).
    void import("$lib/stores/artifacts").then(({ requestChatInputAppend }) => {
      requestChatInputAppend(`/${cmdId} `);
    });
  }

  function handleSessionCreated(session: ChatSessionSummary): void {
    selectedSessionId = session.id;
    showNewChatPicker = false;
  }

  function navigateToSession(sessionId: string) {
    selectedSessionId = sessionId;
    markSessionRead(sessionId);
  }
  function closeConversation() { selectedSessionId = null; }

  async function handleDeleteSession(sessionId: string): Promise<void> {
    // Optimistic update: remove from store immediately
    chatSessions.update((sessions) => sessions.filter((s) => s.id !== sessionId));
    if (selectedSessionId === sessionId) selectedSessionId = null;
    try { await invoke("delete_chat_session", { sessionId }); }
    catch (err: unknown) { console.warn("delete_chat_session failed:", err); }
  }

  async function handleRenameSession(sessionId: string, title: string): Promise<void> {
    // Optimistic update: patch title in store immediately
    chatSessions.update((sessions) =>
      sessions.map((s) => (s.id === sessionId ? { ...s, title } : s))
    );
    try { await invoke("rename_chat_session", { sessionId, title }); }
    catch (err: unknown) { console.warn("rename_chat_session failed:", err); }
  }
</script>

<!-- Chat-route command palette enrichment + Help dialog (US-SP42-033). -->
<CommandPalette
  onselectSession={openSessionFromPalette}
  onnewChat={openNewChatPicker}
  onslashCommand={appendSlashCommand}
/>
<ShortcutsHelpDialog bind:open={showShortcutsHelp} />

<!-- Runtime health banner (US-SP42-034) — stays mounted across sub-states. -->
<RuntimeDisconnectedBanner />

<div class="mx-auto w-full max-w-6xl" data-testid="chat-page">
  <!-- Header -->
  <div class="relative flex items-end justify-between overflow-hidden rounded-2xl bg-gradient-surface px-5 py-5 shadow-elev-1">
    <div class="pointer-events-none absolute inset-0 bg-gradient-accent opacity-60" aria-hidden="true"></div>
    <div class="relative">
      <h1 class="text-display-lg text-foreground" data-testid="chat-header">{$t("chat.title")}</h1>
      <p class="mt-2 text-sm text-muted-foreground md:text-base" data-testid="chat-subtitle">{$t("chat.subtitle")}</p>
    </div>
    <Button size="sm" onclick={() => openNewChatPicker()} data-testid="new-chat-button" class="relative gap-1.5">
      <Plus size={13} />
      {$t("chat.new_chat")}
    </Button>
  </div>

  <!-- New chat picker -->
  {#if showNewChatPicker}
    <div class="mt-4" data-testid="new-chat-picker">
      <QuickPicker
        oncreated={handleSessionCreated}
        onclose={closeNewChatPicker}
      />
    </div>
  {/if}


  <!-- Main content -->
  <div class="mt-4">
    {#if $connectionStatus === "connecting"}
      <div class="space-y-1">
        {#each { length: 4 } as _}
          <div class="flex items-center gap-3 rounded-lg px-3 py-2.5">
            <Skeleton variant="avatar" class="h-3.5 w-3.5 rounded" />
            <Skeleton variant="text" class="h-3.5 w-28" />
            <Skeleton variant="text" class="h-3 flex-1" />
            <Skeleton variant="text" class="h-3 w-12" />
          </div>
        {/each}
      </div>
    {:else if $activeChatSessions.length === 0 && $closedChatSessions.length === 0 && !showNewChatPicker && !selectedSessionId}
      <EmptySessionsState onnewChat={() => openNewChatPicker()} />
    {:else if selectedSessionId}
      <!-- 3-column shell (US-SP42-022): Sessions / Conversation / ContextDrawer. -->
      <div class="overflow-hidden rounded-lg glass-border border" style="height: calc(100vh - 180px);">
        <ChatShell>
          {#snippet sessions()}
            <ChatSessionsSidebar
              {selectedSessionId}
              onNewChat={() => openNewChatPicker()}
              onSelect={(id) => { navigateToSession(id); closeSessionsDrawer(); }}
              onDelete={handleDeleteSession}
              onRename={handleRenameSession}
            />
          {/snippet}

          {#snippet conversation()}
            <div class="flex h-full min-h-0 flex-col glass-card">
              <ChatConversation
                sessionId={selectedSessionId!}
                onclose={closeConversation}
                onconfigtoggle={toggleContextDrawer}
                onsessionsopen={openSessionsDrawer}
                ondelete={handleDeleteSession}
                onnewChat={() => { closeConversation(); openNewChatPicker(); }}
              />
            </div>
          {/snippet}

          {#snippet context()}
            <ContextDrawer
              session={$currentSession}
              onupdated={() => { /* ChatConversation subscribes to runtime events */ }}
              onclose={() => contextDrawerOpen.set(false)}
            />
          {/snippet}
        </ChatShell>
      </div>
    {:else}
      <!-- Session list (no session selected) — card list style -->
      {@const activeDecorated = $decoratedSessions.filter((s) => !s.archived && s.status !== "closed")}
      {@const closedDecorated = $decoratedSessions.filter((s) => !s.archived && s.status === "closed")}
      {#if activeDecorated.length > 0}
        <p class="px-1 pb-2 text-[10px] font-semibold uppercase tracking-wider text-primary/50" data-testid="chat-active-section">
          {$t("chat.active_sessions")}
        </p>
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/20 shadow-sm" data-testid="chat-active-list">
          {#each activeDecorated as session (session.id)}
            <ChatSessionCard
              {session}
              onclick={navigateToSession}
              ondelete={handleDeleteSession}
              onrename={handleRenameSession}
            />
          {/each}
        </div>
      {/if}

      {#if closedDecorated.length > 0}
        <p class="px-1 pt-4 pb-2 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/40" data-testid="chat-closed-section">
          {$t("chat.closed_sessions")}
        </p>
        <div class="glass-card glass-border rounded-lg overflow-hidden divide-y divide-border/20" data-testid="chat-closed-list">
          {#each closedDecorated as session (session.id)}
            <ChatSessionCard
              {session}
              onclick={navigateToSession}
              ondelete={handleDeleteSession}
              onrename={handleRenameSession}
            />
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>
