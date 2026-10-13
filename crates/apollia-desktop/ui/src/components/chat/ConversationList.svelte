<!--
  The conversations of the Chat list. Without a search: "New folder", the
  folders with the conversations each holds, then the conversations out of
  any folder. With a search: the conversations found by their title, then the
  ones found in their messages, each under the passage that matched.
-->
<script module lang="ts">
  import type { ConversationState } from "$lib/components/operator";
  import type { DecoratedSession } from "$lib/stores/chatSessions";

  /** How long ago, in the short form the rows show. */
  export function relativeTime(iso: string | null | undefined, now: number = Date.now()): string {
    if (!iso) return "-";
    const d = new Date(iso).getTime();
    if (Number.isNaN(d)) return "-";
    const diffSec = Math.max(1, Math.round((now - d) / 1000));
    if (diffSec < 60) return `${diffSec}s`;
    const min = Math.round(diffSec / 60);
    if (min < 60) return `${min}min`;
    const h = Math.round(min / 60);
    if (h < 24) return `${h}h`;
    const day = Math.round(h / 24);
    return `${day}j`;
  }

  /** The state a row shows, the selected one first. */
  export function rowState(s: DecoratedSession, isSelected: boolean): ConversationState {
    if (isSelected) return "active";
    if (s.archived) return "archived";
    if (s.status === "closed") return "closed";
    if (s.pinned) return "pinned";
    if (s.unread) return "unread";
    return "default";
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { chatDoubleClickRename } from "$lib/stores/chatBehaviours";
  import { chatTokenCount, formatTokenCount } from "$lib/stores/chatTokenCount";
  import { FolderPlus } from "lucide-svelte";
  import { ConversationRow } from "$lib/components/operator";
  import type { SidebarDensity } from "$lib/stores/chatSessions";
  import {
    dropTargetOf,
    refusesDrag,
    startsDrag,
    wouldMove,
    type DropTarget,
    type Point,
  } from "$lib/chat/sessionDrag";
  import {
    chatFolders,
    collapsedFolderIds,
    createFolder,
    deleteFolder,
    folderIdOf,
    groupByFolder,
    loadFolders,
    moveSessionToFolder,
    renameFolder,
    toggleFolderCollapsed,
  } from "$lib/stores/chatFolders";
  import {
    clearMessageSearch,
    hitsFor,
    messageSearch,
    searchMessagesFor,
    searchSections,
    searchesMessages,
  } from "$lib/stores/chatSearch";
  import ConversationFolderRow from "./ConversationFolderRow.svelte";

  interface Props {
    /** The conversations the list shows, filtered by title and in order. */
    sessions: DecoratedSession[];
    /** Every conversation the list could show, whatever the search. */
    listed: DecoratedSession[];
    /** What is typed in the search field. */
    query: string;
    selectedSessionId: string | null;
    density: SidebarDensity;
    onselect: (sessionId: string) => void;
    onrename: (sessionId: string, title: string) => void;
    ondelete: (sessionId: string) => void;
    onconfigure: (sessionId: string) => void;
  }

  let {
    sessions,
    listed,
    query,
    selectedSessionId,
    density,
    onselect,
    onrename,
    ondelete,
    onconfigure,
  }: Props = $props();

  let failure = $state<string | null>(null);
  // The folder that was just created: its row opens on its name.
  let namingFolderId = $state<string | null>(null);
  // A press on a conversation, which becomes a drag once the pointer travels.
  let press = $state<{ sessionId: string; title: string; from: Point } | null>(null);
  let dragging = $state(false);
  let pointer = $state<Point>({ x: 0, y: 0 });
  let over = $state<DropTarget | null>(null);

  onMount(() => {
    void loadFolders();
    return clearMessageSearch;
  });

  const searching = $derived(query.trim() !== "");
  $effect(() => {
    searchMessagesFor(query);
  });

  const grouped = $derived(groupByFolder(sessions, $chatFolders));
  const moveTargets = $derived($chatFolders.map(({ id, name }) => ({ id, name })));
  const sections = $derived(searchSections(sessions, hitsFor($messageSearch, query), listed));
  const searchPending = $derived(
    searchesMessages(query) && $messageSearch.query !== query.trim(),
  );

  async function run(action: () => Promise<unknown>): Promise<void> {
    failure = null;
    try {
      await action();
    } catch (err: unknown) {
      failure = err instanceof Error ? err.message : String(err);
    }
  }

  async function newFolder(): Promise<void> {
    await run(async () => {
      const folder = await createFolder($t("chat.folders.default_name"));
      namingFolderId = folder.id;
    });
  }

  function move(sessionId: string, folderId: string | null): void {
    if (folderIdOf($chatFolders, sessionId) === folderId) return;
    void run(() => moveSessionToFolder(sessionId, folderId));
  }

  /** The place the dragged conversation would move to, or `null`. */
  const landing = $derived.by(() => {
    if (!dragging || !press) return null;
    return wouldMove(over, folderIdOf($chatFolders, press.sessionId)) ? over : null;
  });

  function onRowPointerDown(ev: PointerEvent, session: DecoratedSession): void {
    if (searching || ev.button !== 0 || refusesDrag(ev.target)) return;
    press = {
      sessionId: session.id,
      title: session.title ?? session.agent_name ?? $t("chat.untitled_session"),
      from: { x: ev.clientX, y: ev.clientY },
    };
  }

  function onPointerMove(ev: PointerEvent): void {
    if (!press) return;
    const at = { x: ev.clientX, y: ev.clientY };
    if (!dragging && !startsDrag(press.from, at)) return;
    dragging = true;
    pointer = at;
    over = dropTargetOf(document.elementFromPoint(at.x, at.y));
    // The pointer is moving a conversation, not selecting the text it passes.
    ev.preventDefault();
  }

  function endDrag(): void {
    press = null;
    dragging = false;
    over = null;
  }

  function onPointerUp(): void {
    const dropped = press;
    const target = landing;
    const wasDragging = dragging;
    endDrag();
    if (!wasDragging || !dropped) return;
    // The release of a drag is not a click on the row under the pointer.
    window.addEventListener("click", swallowClick, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallowClick, { capture: true }), 0);
    if (target) move(dropped.sessionId, target.folderId);
  }

  function swallowClick(ev: MouseEvent): void {
    ev.stopPropagation();
    ev.preventDefault();
  }

  function onKeydown(ev: KeyboardEvent): void {
    if (dragging && ev.key === "Escape") endDrag();
  }
</script>

<svelte:window
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={endDrag}
  onkeydown={onKeydown}
/>

{#snippet row(session: DecoratedSession, excerpt: string | undefined)}
  <div
    onpointerdown={(ev) => onRowPointerDown(ev, session)}
    class={dragging && press?.sessionId === session.id ? "opacity-50" : ""}
    role="listitem"
    data-testid="chat-conversation-item"
    data-session-id={session.id}
  >
    <ConversationRow
      title={session.title ?? session.agent_name ?? $t("chat.untitled_session")}
      lastMessage={session.last_message_preview ?? undefined}
      timestamp={relativeTime(session.lastActivityAt)}
      state={rowState(session, session.id === selectedSessionId)}
      live={session.status === "processing"}
      onclick={() => onselect(session.id)}
      onrename={(newTitle) => onrename(session.id, newTitle)}
      ondelete={() => ondelete(session.id)}
      onconfigure={() => onconfigure(session.id)}
      {density}
      {excerpt}
      folders={moveTargets}
      folderId={folderIdOf($chatFolders, session.id)}
      onmove={(folderId) => move(session.id, folderId)}
      renameOnDoubleClick={$chatDoubleClickRename}
      tokens={$chatTokenCount
        ? $t("chat.conversation_tokens", {
            values: { count: formatTokenCount(session.output_tokens ?? 0, $locale ?? "en") },
          })
        : undefined}
    />
  </div>
{/snippet}

{#if failure}
  <p
    class="mx-3 my-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-caption text-destructive"
    role="alert"
  >
    {$t("chat.folders.action_failed", { values: { error: failure } })}
  </p>
{/if}

{#if searching}
  {#if sections.titles.length > 0}
    <h3 class="px-3.5 pb-1 pt-2 text-micro-lg text-muted-foreground" data-testid="chat-search-titles">
      {$t("chat.search.titles")}
    </h3>
    <div role="list">
      {#each sections.titles as session (session.id)}
        {@render row(session, undefined)}
      {/each}
    </div>
  {/if}
  {#if sections.messages.length > 0}
    <h3 class="px-3.5 pb-1 pt-2 text-micro-lg text-muted-foreground" data-testid="chat-search-messages">
      {$t("chat.search.in_messages")}
    </h3>
    <div role="list">
      {#each sections.messages as match (match.session.id)}
        {@render row(match.session, match.excerpt)}
      {/each}
    </div>
  {/if}
  {#if $messageSearch.failed && $messageSearch.query === query.trim()}
    <p class="px-4 py-3 text-center text-caption text-muted-foreground">
      {$t("chat.search.failed")}
    </p>
  {:else if sections.titles.length === 0 && sections.messages.length === 0}
    <p class="px-4 py-6 text-center text-caption text-muted-foreground" data-testid="chat-search-empty">
      {searchPending ? $t("chat.search.pending") : $t("chat.no_results")}
    </p>
  {/if}
{:else}
  <button
    type="button"
    class="flex w-full items-center gap-2 px-3.5 py-2 text-left text-body-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60"
    onclick={() => void newFolder()}
    data-testid="chat-new-folder-btn"
  >
    <FolderPlus size={13} strokeWidth={1.75} aria-hidden="true" />
    {$t("chat.folders.new")}
  </button>

  {#each grouped.folders as group (group.folder.id)}
    {@const collapsed = $collapsedFolderIds.has(group.folder.id)}
    <ConversationFolderRow
      folderId={group.folder.id}
      name={group.folder.name}
      count={group.sessions.length}
      {collapsed}
      renaming={namingFolderId === group.folder.id}
      ontoggle={() => toggleFolderCollapsed(group.folder.id)}
      onrename={(name) => void run(() => renameFolder(group.folder.id, name))}
      ondelete={() => void run(() => deleteFolder(group.folder.id))}
      dropTarget={landing?.folderId === group.folder.id}
      onrenamed={() => (namingFolderId = null)}
      renameOnDoubleClick={$chatDoubleClickRename}
    />
    {#if !collapsed}
      {#if group.sessions.length === 0}
        <p class="px-3.5 pb-2 pl-10 text-micro-lg text-muted-foreground" data-testid="chat-folder-empty">
          {$t("chat.folders.empty")}
        </p>
      {:else}
        <div
          role="list"
          class="transition-colors {landing?.folderId === group.folder.id ? 'bg-primary/5' : ''}"
          data-session-drop={group.folder.id}
          data-testid="chat-folder-sessions"
          data-folder-id={group.folder.id}
        >
          {#each group.sessions as session (session.id)}
            {@render row(session, undefined)}
          {/each}
        </div>
      {/if}
    {/if}
  {/each}

  <div
    role="list"
    class="min-h-10 flex-1 transition-colors {landing && landing.folderId === null
      ? 'bg-primary/5'
      : ''}"
    data-session-drop=""
    data-testid="chat-loose-sessions"
  >
    {#each grouped.loose as session (session.id)}
      {@render row(session, undefined)}
    {/each}
    {#if sessions.length === 0}
      <p class="px-4 py-6 text-center text-caption text-muted-foreground">
        {$t("chat.no_sessions")}
      </p>
    {/if}
  </div>
{/if}

{#if dragging && press}
  <!-- What is being moved, carried next to the pointer. -->
  <div
    class="pointer-events-none fixed max-w-56 truncate rounded-md border border-border bg-surface-1 px-2 py-1 text-body-xs text-foreground shadow-md"
    style="z-index: var(--z-overlay); left: {pointer.x + 12}px; top: {pointer.y + 12}px;"
    data-testid="chat-session-drag-ghost"
  >
    {press.title}
  </div>
{/if}
