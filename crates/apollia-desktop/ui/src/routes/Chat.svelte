<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "svelte-i18n";
  import { Plus, Search } from "lucide-svelte";
  import { connectionStatus } from "$lib/stores/sse";
  import {
    activeChatSessions,
    closedChatSessions,
    pendingChatSessionId,
    openNewChatRequested,
  } from "$lib/stores/chat";
  import { chatSessions } from "$lib/stores/sse";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import type { ChatSessionSummary, ProjectSummary } from "$lib/types";
  import { deleteChatSession, renameChatSession } from "$lib/ipc/chat";
  import { listProjects } from "$lib/ipc/projects";
  import ChatConversation from "../components/chat/ChatConversation.svelte";
  import EmptySessionsState from "../components/chat/EmptySessionsState.svelte";
  import QuickPicker from "../components/chat/QuickPicker.svelte";
  import ChatConfigPanelBody from "../components/chat/ChatConfigPanelBody.svelte";
  import {
    decoratedSessions,
    markSessionRead,
    type DecoratedSession,
  } from "$lib/stores/chatSessions";
  import {
    toggleSessionsSidebar,
    openSessionsDrawer,
    closeSessionsDrawer,
    chatViewport,
    sessionsDrawerOpen,
    contextDrawerOpen,
    toggleContextDrawer,
  } from "$lib/stores/chatLayout";
  import { currentSession } from "$lib/stores/chat";
  import type { ChatMessageView } from "$lib/types";
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

  // Operator design-system primitives - V3 Chat Refonte v2.
  import {
    ConversationRow,
    Journal,
    PlanDagPanel,
    type JournalEvent,
    type JournalMode,
    type ConversationState,
  } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";

  let selectedSessionId = $state<string | null>(null);
  let showNewChatPicker = $state(false);
  let showShortcutsHelp = $state(false);
  let sessionSearchQuery = $state("");
  let journalMode = $state<JournalMode>("operator");
  let railTab = $state<"journal" | "plan" | "config">("journal");

  // Open the inspector on the Config tab (from a row "..." action or the header
  // gear). The config panel reads the active session via `currentSession`.
  function openConfig(sessionId?: string): void {
    if (sessionId && sessionId !== selectedSessionId) navigateToSession(sessionId);
    railTab = "config";
    contextDrawerOpen.set(true);
  }

  // Responsive shell: both side panes are inline columns at lg+, and collapse
  // to toggleable overlay drawers below lg so the conversation stays readable.
  const panesInline = $derived($chatViewport === "lg");

  // ── Shortcut registry ──────────────────────────────
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
        // components - fall through so they keep working.
        return false;
      },
    },
  ];

  // Projects loaded once so we can render a small "Project X" chip on
  // each conversation row (mirrors the badge in the open conversation
  // header). Refreshed when the user creates/links sessions elsewhere.
  let projectList = $state<ProjectSummary[]>([]);
  const projectNameById = $derived(
    new Map(projectList.map((p) => [p.id, p.name])),
  );
  async function reloadProjectList(): Promise<void> {
    try {
      projectList = await listProjects();
    } catch {
      // Non-blocking - the chip just won't render.
    }
  }

  onMount(() => {
    void reloadProjectList();
    const unsub = pendingChatSessionId.subscribe((id) => {
      if (id) { selectedSessionId = id; pendingChatSessionId.set(null); }
    });
    const uninstall = installChatShortcuts(SHORTCUT_BINDINGS);
    const unregisterHelp = registerShortcuts(SHORTCUT_BINDINGS.map(describeBinding));
    return () => {
      unsub();
      uninstall();
      unregisterHelp();
    };
  });

  // The preset payload is currently informational - QuickPicker has no
  // preset prop yet. We accept and
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
    showNewChatPicker = false;
    selectedSessionId = sessionId;
    markSessionRead(sessionId);
  }
  function closeConversation() { selectedSessionId = null; }

  async function handleDeleteSession(sessionId: string): Promise<void> {
    try {
      await deleteChatSession(sessionId);
    } catch (err: unknown) {
      console.error("delete_chat_session failed:", err);
      return;
    }
    chatSessions.update((sessions) => sessions.filter((s) => s.id !== sessionId));
    if (selectedSessionId === sessionId) selectedSessionId = null;
  }

  async function handleRenameSession(sessionId: string, newTitle: string): Promise<void> {
    const trimmed = newTitle.trim();
    if (!trimmed) return;
    try {
      await renameChatSession(sessionId, trimmed);
      // Optimistic update so the row reflects the new title before the next SSE refresh.
      chatSessions.update((sessions) =>
        sessions.map((s) => (s.id === sessionId ? { ...s, title: trimmed } : s)),
      );
    } catch (err: unknown) {
      console.warn("rename_chat_session failed:", err);
    }
  }

  // ─── Sidebar list (V3 Operator) ───────────────────────────────────────────
  // Re-uses the existing `decoratedSessions` store: the SSE wiring stays
  // owned by `chatSessions` upstream - this view is purely presentational.

  function relativeTime(iso: string | null | undefined): string {
    if (!iso) return "-";
    const d = new Date(iso).getTime();
    if (Number.isNaN(d)) return "-";
    const diffSec = Math.max(1, Math.round((Date.now() - d) / 1000));
    if (diffSec < 60) return `${diffSec}s`;
    const min = Math.round(diffSec / 60);
    if (min < 60) return `${min}min`;
    const h = Math.round(min / 60);
    if (h < 24) return `${h}h`;
    const day = Math.round(h / 24);
    return `${day}j`;
  }

  function rowState(s: DecoratedSession, isSelected: boolean): ConversationState {
    if (isSelected) return "active";
    if (s.archived) return "archived";
    if (s.status === "closed") return "closed";
    if (s.pinned) return "pinned";
    if (s.unread) return "unread";
    return "default";
  }

  const filteredSessions = $derived(
    $decoratedSessions
      .filter((s) => !s.archived)
      .filter((s) => {
        if (!sessionSearchQuery.trim()) return true;
        const q = sessionSearchQuery.toLowerCase();
        const hay = [
          s.title ?? "",
          s.agent_name ?? "",
          s.last_message_preview ?? "",
          s.mode,
        ].join(" ").toLowerCase();
        return hay.includes(q);
      }),
  );

  // Live agent activity events for the right Journal panel.
  // Source: `currentSession` (set by ChatConversation.applySessionDetail on every
  // refresh - both initial load and runtime-event-triggered refreshes), so the
  // Journal updates live as the SSE stream lands new messages.
  function fmtTime(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return "";
    return d.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
  }

  function previewText(s: string, max = 160): string {
    const collapsed = s.replace(/\s+/g, " ").trim();
    return collapsed.length > max ? collapsed.slice(0, max - 1) + "…" : collapsed;
  }

  function buildJournalFromMessages(
    messages: ChatMessageView[],
    mode: JournalMode,
  ): JournalEvent[] {
    const isBuilder = mode === "builder";
    const out: JournalEvent[] = [];
    for (const m of messages) {
      const time = fmtTime(m.created_at);
      // Tool calls - surface each one as a dedicated tool event.
      const calls = m.tool_calls ?? [];
      for (const tc of calls) {
        const status = tc.status;
        const heading = isBuilder
          ? tc.tool_name
          : $t("chat.journal.tool_heading", { values: { name: tc.tool_name } });
        let body: string;
        if (isBuilder) {
          const inputJson = (() => {
            try { return JSON.stringify(tc.input ?? {}); } catch { return "{}"; }
          })();
          const outputStr = tc.output ?? "";
          body = `input=${previewText(inputJson, 240)}${outputStr ? ` · output=${previewText(outputStr, 240)}` : ""}${tc.duration_ms != null ? ` · ${tc.duration_ms}ms` : ""}`;
        } else {
          body = status === "executed"
            ? $t("chat.journal.tool_executed")
            : status === "refused"
              ? $t("chat.journal.tool_refused")
              : status === "authorized"
                ? $t("chat.journal.tool_authorized")
                : $t("chat.journal.tool_pending");
        }
        const evType: JournalEvent["type"] = status === "pending" || status === "authorized" ? "wait" : "tool";
        out.push({ type: evType, heading, body, time });
      }

      // Message body - skip empty content (tool-only assistant turns).
      const content = (m.content ?? "").trim();
      if (!content) continue;
      switch (m.role) {
        case "user":
          out.push({
            type: "msg",
            // i18n-ignore: builder skin shows the raw runtime role
            heading: isBuilder ? "user" : $t("chat.journal.you"),
            body: previewText(content, isBuilder ? 400 : 200),
            time,
          });
          break;
        case "assistant": {
          const meta = m.metadata ?? {};
          const isHitl = Boolean(meta.hitl) || Boolean(meta.approval);
          out.push({
            type: isHitl ? "hitl" : "msg_out",
            // i18n-ignore: builder skin shows the raw runtime role
            heading: isBuilder ? "assistant" : $t("chat.journal.agent"),
            body: previewText(content, isBuilder ? 400 : 200),
            time,
          });
          break;
        }
        case "tool":
          if (isBuilder) {
            out.push({
              type: "tool",
              // i18n-ignore: builder skin shows the raw runtime role
              heading: m.tool_name ?? "tool",
              body: previewText(content, 400),
              time,
            });
          }
          // Operator mode already surfaces a friendly tool event from tool_calls above.
          break;
        case "system":
          out.push({
            type: "err",
            // i18n-ignore: builder skin shows the raw runtime role
            heading: isBuilder ? "system" : $t("chat.journal.system"),
            body: previewText(content, 240),
            time,
          });
          break;
      }
    }
    return out;
  }

  const journalEvents = $derived<JournalEvent[]>(
    $currentSession && $currentSession.id === selectedSessionId
      ? buildJournalFromMessages($currentSession.messages ?? [], journalMode)
      : [],
  );
</script>

<!-- Chat-route command palette enrichment + Help dialog. -->
<CommandPalette
  onselectSession={openSessionFromPalette}
  onnewChat={openNewChatPicker}
  onslashCommand={appendSlashCommand}
/>
<ShortcutsHelpDialog bind:open={showShortcutsHelp} />

<!--
  V3 Operator chat layout: left sessions rail / center thread / right journal.
  PageHeader is intentionally absent - the route fills the panel and the
  page title lives in the Topbar (per V3 ChatPage).
-->
<div
  class="relative flex h-full min-h-0 w-full overflow-hidden bg-background"
  data-testid="chat-page"
>
  <!-- Drawer backdrops (below lg only). -->
  {#if !panesInline && $sessionsDrawerOpen}
    <button
      type="button"
      class="absolute inset-0 z-30 bg-foreground/20"
      aria-label={$t("a11y.close")}
      onclick={closeSessionsDrawer}
      data-testid="chat-sessions-backdrop"
    ></button>
  {/if}
  {#if !panesInline && $contextDrawerOpen}
    <button
      type="button"
      class="absolute inset-0 z-30 bg-foreground/20"
      aria-label={$t("a11y.close")}
      onclick={toggleContextDrawer}
      data-testid="chat-context-backdrop"
    ></button>
  {/if}

  <!-- ── Left rail: conversations ─────────────────────────────────────── -->
  <aside
    class="flex h-full flex-col border-r border-border bg-card {panesInline
      ? 'w-[280px] shrink-0'
      : 'absolute inset-y-0 left-0 z-40 w-[280px] max-w-[85%] shadow-elev-3 transition-transform duration-200 ' +
        ($sessionsDrawerOpen ? 'translate-x-0' : '-translate-x-full')}"
    aria-label={$t("chat.conversations_aria")}
    aria-hidden={!panesInline && !$sessionsDrawerOpen}
  >
    <div class="px-3.5 pt-4 pb-2">
      <h2
        class="mb-3 text-[16px] font-semibold tracking-[-0.2px] text-foreground"
        data-testid="chat-header"
      >
        {$t("chat.title")}
      </h2>

      <div class="relative mb-2.5">
        <Search
          size={12}
          class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          type="search"
          bind:value={sessionSearchQuery}
          placeholder={$t("chat.search_placeholder")}
          aria-label={$t("chat.search_placeholder")}
          data-testid="session-search-input"
          class="h-8 w-full rounded-md border border-border bg-background pl-7 pr-2 text-[12px] text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/40"
         />
      </div>

      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => openNewChatPicker()}
        data-testid="chat-new-chat-btn"
      >
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("chat.new_chat")}
      </Button>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto" data-testid="chat-active-list">
      {#if $connectionStatus === "connecting"}
        <div class="space-y-1 px-3 py-2">
          {#each { length: 4 } as _}
            <div class="flex items-center gap-3 rounded-lg px-3 py-2.5">
              <Skeleton variant="avatar" class="h-3.5 w-3.5 rounded" />
              <Skeleton variant="text" class="h-3.5 w-28" />
              <Skeleton variant="text" class="h-3 flex-1" />
            </div>
          {/each}
        </div>
      {:else if filteredSessions.length === 0}
        <div class="px-4 py-6 text-center text-[11px] text-muted-foreground">
          {#if sessionSearchQuery.trim()}
            {$t("chat.no_results")}
          {:else}
            {$t("chat.no_sessions")}
          {/if}
        </div>
      {:else}
        {#each filteredSessions as session (session.id)}
          <ConversationRow
            title={session.title ?? session.agent_name ?? $t("chat.untitled_session")}
            lastMessage={session.last_message_preview ?? undefined}
            timestamp={relativeTime(session.lastActivityAt)}
            state={rowState(session, session.id === selectedSessionId)}
            live={session.status === "processing"}
            projectLabel={session.project_id
              ? (projectNameById.get(session.project_id) ?? undefined)
              : undefined}
            onclick={() => { navigateToSession(session.id); closeSessionsDrawer(); }}
            onrename={(newTitle) => void handleRenameSession(session.id, newTitle)}
            ondelete={() => void handleDeleteSession(session.id)}
            onconfigure={() => { openConfig(session.id); closeSessionsDrawer(); }}
          />
        {/each}
      {/if}
    </div>
  </aside>

  <!-- ── Center: thread + composer ─────────────────────────────────────── -->
  <main class="relative flex h-full min-h-0 min-w-0 flex-1 flex-col bg-background">
    {#if showNewChatPicker}
      <!-- Picker fills the column and scrolls internally; mutually exclusive
           with the active conversation so they never stack. -->
      <div
        class="flex min-h-0 flex-1 flex-col overflow-y-auto px-6 py-4"
        data-testid="new-chat-picker"
      >
        <QuickPicker
          oncreated={handleSessionCreated}
          onclose={closeNewChatPicker}
        />
      </div>
    {:else if $connectionStatus === "connecting" && !selectedSessionId}
      <div class="flex flex-1 items-center justify-center">
        <div class="flex flex-col items-center gap-3 text-muted-foreground">
          <Skeleton variant="text" class="h-3 w-32" />
          <Skeleton variant="text" class="h-3 w-24" />
        </div>
      </div>
    {:else if selectedSessionId}
      <!--
        ChatConversation owns the message thread, SSE subscriptions,
        composer, slash commands, mention handling, and the markdown/code
        rendering pipeline. We drop it directly into the center column so
        all that wiring is preserved verbatim.
      -->
      <div class="flex h-full min-h-0 flex-1 flex-col">
        <ChatConversation
          sessionId={selectedSessionId}
          onclose={closeConversation}
          onsessionsopen={openSessionsDrawer}
          oncontextopen={toggleContextDrawer}
          onconfigtoggle={() => openConfig()}
          ondelete={handleDeleteSession}
          onnewChat={() => { closeConversation(); openNewChatPicker(); }}
        />
      </div>
    {:else if $activeChatSessions.length === 0 && $closedChatSessions.length === 0}
      <div class="flex flex-1 items-center justify-center px-6 py-10">
        <EmptySessionsState onnewChat={() => openNewChatPicker()} />
      </div>
    {:else}
      <!-- No session selected but list is non-empty: prompt the user. -->
      <div class="flex flex-1 items-center justify-center px-6 py-10">
        <div class="max-w-sm text-center">
          <h3
            class="mb-3 text-[20px] font-semibold tracking-[-0.3px] text-foreground"
          >
            {$t("chat.subtitle")}
          </h3>
          <p class="mb-5 text-[12.5px] text-muted-foreground">
            {$t("chat.select_or_start")}
          </p>
          <div class="inline-flex">
            <Button variant="primary-solid" size="sm" onclick={() => openNewChatPicker()}>
              {#snippet icon()}<Plus size={12} />{/snippet}
              {$t("chat.new_chat")}
            </Button>
          </div>
        </div>
      </div>
    {/if}
  </main>

  <!-- ── Inspector (on-demand): live journal / plan. Closed by default at every
       width so the conversation stays the hero; opens inline (docked) at lg and
       as an overlay drawer below lg. ───────────────────────────────────────── -->
  {#if $contextDrawerOpen || !panesInline}
  <aside
    class="flex h-full flex-col border-l border-border bg-card {panesInline
      ? 'w-[380px] shrink-0'
      : 'absolute inset-y-0 right-0 z-40 w-[320px] max-w-[85%] shadow-elev-3 transition-transform duration-200 ' +
        ($contextDrawerOpen ? 'translate-x-0' : 'translate-x-full')}"
    aria-label={railTab === "plan" ? $t("plan_session.tab_plan") : $t("plan_session.tab_journal")}
    aria-hidden={!panesInline && !$contextDrawerOpen}
  >
    <div class="flex items-center justify-between border-b border-border px-4 py-3">
      <div class="inline-flex rounded-md border border-border p-0.5 text-[10.5px]" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={railTab === "journal"}
          class="rounded px-2 py-0.5 transition-colors {railTab === 'journal'
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (railTab = "journal")}
        >
          {$t("plan_session.tab_journal")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={railTab === "plan"}
          class="rounded px-2 py-0.5 transition-colors {railTab === 'plan'
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (railTab = "plan")}
          data-testid="chat-inspector-plan-tab"
        >
          {$t("plan_session.tab_plan")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={railTab === "config"}
          class="rounded px-2 py-0.5 transition-colors {railTab === 'config'
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:text-foreground'}"
          onclick={() => (railTab = "config")}
          data-testid="chat-inspector-config-tab"
        >
          {$t("chat.config_title")}
        </button>
      </div>
      {#if railTab === "journal"}
        <div class="inline-flex rounded-md border border-border p-0.5 text-[10.5px]">
          <button
            type="button"
            class="rounded px-2 py-0.5 transition-colors {journalMode === 'operator'
              ? 'bg-primary/10 text-primary'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (journalMode = "operator")}
          >
            {$t("chat.journal_mode_operator")}
          </button>
          <button
            type="button"
            class="rounded px-2 py-0.5 transition-colors {journalMode === 'builder'
              ? 'bg-primary/10 text-primary'
              : 'text-muted-foreground hover:text-foreground'}"
            onclick={() => (journalMode = "builder")}
          >
            {$t("chat.journal_mode_builder")}
          </button>
        </div>
      {/if}
    </div>

    {#if railTab === "config"}
      <div class="min-h-0 flex-1 overflow-y-auto" data-testid="chat-inspector-config">
        {#if $currentSession && $currentSession.id === selectedSessionId}
          <ChatConfigPanelBody
            session={$currentSession}
            showHeader={false}
            onupdated={() => {}}
          />
        {:else}
          <div class="px-4 py-6 text-center text-[11px] text-muted-foreground">
            {$t("chat.subtitle")}
          </div>
        {/if}
      </div>
    {:else if railTab === "plan" && selectedSessionId}
      <div class="min-h-0 flex-1">
        {#key selectedSessionId}
          <PlanDagPanel sessionId={selectedSessionId} />
        {/key}
      </div>
    {:else}
      <div class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {#if journalEvents.length === 0}
          <div
            class="rounded-[10px] border border-dashed border-border px-4 py-6 text-center text-[11px] text-muted-foreground"
          >
            {$t("chat.journal_empty")}
          </div>
        {:else}
          <Journal events={journalEvents} mode={journalMode} />
        {/if}
      </div>
    {/if}

  </aside>
  {/if}
</div>
