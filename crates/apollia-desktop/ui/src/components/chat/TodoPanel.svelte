<script lang="ts" module>
  import type { TodoItem, TodoStatus, TodoUpdatedPayload } from "$lib/ipc/todo";

  /** Status buckets rendered as distinct sections, in display order. */
  export const TODO_SECTION_ORDER: TodoStatus[] = [
    "in_progress",
    "pending",
    "completed",
  ];

  /** Groups items by status, preserving insertion order within each bucket. */
  export function bucketTodos(items: TodoItem[]): Record<TodoStatus, TodoItem[]> {
    return {
      pending: items.filter((i) => i.status === "pending"),
      in_progress: items.filter((i) => i.status === "in_progress"),
      completed: items.filter((i) => i.status === "completed"),
    };
  }

  /** Normalizes an unknown thrown value into a display string. */
  export function todoErrorMessage(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  /**
   * Resolves a live `"todo-updated"` payload against the panel's session.
   *
   * Returns the new snapshot when the payload targets `sessionId`, or `null`
   * when it belongs to another open session and must be ignored.
   */
  export function resolveTodoUpdate(
    payload: TodoUpdatedPayload,
    sessionId: string,
  ): TodoItem[] | null {
    return payload.session_id === sessionId ? payload.items : null;
  }
</script>

<script lang="ts">
  /**
   * `TodoPanel` - live todo list for the current chat session.
   *
   * Initial state is read once via `getSessionTodo`; subsequent changes arrive
   * through the dedicated `"todo-updated"` Tauri event, filtered by session.
   * The listener is always unsubscribed on teardown.
   *
   * Two skins, inherited from `$uiMode` unless overridden:
   * - `operator`: condensed, status icon plus the item label.
   * - `builder`:  extended, also shows the item `id` and its `depends_on`.
   */
  import { t } from "svelte-i18n";
  import { listen } from "@tauri-apps/api/event";
  import { CheckCircle2, Circle, LoaderCircle, AlertCircle } from "lucide-svelte";
  import { getSessionTodo } from "$lib/ipc/todo";
  import { uiMode, type UIMode } from "$lib/stores/mode";

  interface Props {
    sessionId: string;
    /** Override the global `$uiMode`; otherwise inherits it. */
    skin?: UIMode | undefined;
  }

  let { sessionId, skin = undefined }: Props = $props();

  const effectiveSkin = $derived<UIMode>(skin ?? $uiMode);

  let todoItems = $state<TodoItem[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let connectionError = $state<string | null>(null);

  const buckets = $derived(bucketTodos(todoItems));
  const isEmpty = $derived(todoItems.length === 0 && !loading && !loadError);

  // Initial one-shot read for the current session.
  $effect(() => {
    const target = sessionId;
    loading = true;
    loadError = null;
    getSessionTodo(target)
      .then((items) => {
        todoItems = items;
      })
      .catch((err: unknown) => {
        loadError = todoErrorMessage(err);
      })
      .finally(() => {
        loading = false;
      });
  });

  // Live updates with guaranteed cleanup. A listener failure surfaces a
  // connection error without clearing the already-loaded items.
  $effect(() => {
    const target = sessionId;
    let unlisten: (() => void) | null = null;
    listen<TodoUpdatedPayload>("todo-updated", (event) => {
      const next = resolveTodoUpdate(event.payload, target);
      if (next !== null) {
        todoItems = next;
        connectionError = null;
      }
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err: unknown) => {
        connectionError = todoErrorMessage(err);
      });
    return () => {
      unlisten?.();
    };
  });
</script>

<section class="flex flex-col gap-2 text-sm" data-testid="todo-panel">
  <h3 class="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
    {$t("chat.todo.title")}
  </h3>

  {#if loading}
    <p class="text-muted-foreground" data-testid="todo-loading">
      {$t("chat.todo.loading")}
    </p>
  {:else if loadError}
    <p class="flex items-center gap-2 text-destructive" data-testid="todo-load-error">
      <AlertCircle class="h-4 w-4 shrink-0" />
      <span>{$t("chat.todo.load_error")}: {loadError}</span>
    </p>
  {:else if isEmpty}
    <p class="text-muted-foreground" data-testid="todo-empty-state">
      {$t("chat.todo.empty")}
    </p>
  {:else}
    {#if connectionError}
      <p class="flex items-center gap-2 text-warning" data-testid="todo-connection-error">
        <AlertCircle class="h-4 w-4 shrink-0" />
        <span>{$t("chat.todo.connection_error")}</span>
      </p>
    {/if}

    {#each TODO_SECTION_ORDER as status (status)}
      {#if buckets[status].length > 0}
        <div class="flex flex-col gap-1" data-testid={`todo-section-${status}`}>
          <span class="text-[11px] font-medium uppercase text-muted-foreground">
            {$t(`chat.todo.section_${status}`)}
          </span>
          {#each buckets[status] as item (item.id)}
            <div
              class="flex items-start gap-2 rounded-md border border-border/40 bg-muted/30 px-2 py-1"
              data-testid={`todo-item-${item.id}`}
            >
              {#if item.status === "completed"}
                <CheckCircle2 class="mt-0.5 h-4 w-4 shrink-0 text-success" />
              {:else if item.status === "in_progress"}
                <LoaderCircle class="mt-0.5 h-4 w-4 shrink-0 animate-spin text-warning" />
              {:else}
                <Circle class="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
              {/if}
              <div class="flex flex-col">
                <span
                  class:line-through={item.status === "completed"}
                  class:text-muted-foreground={item.status === "completed"}
                >
                  {item.content}
                </span>
                {#if effectiveSkin === "builder"}
                  <span class="text-[10px] text-muted-foreground">
                    {item.id}{#if item.depends_on.length > 0}
                      {" "}&middot; {item.depends_on.join(", ")}{/if}
                  </span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/each}
  {/if}
</section>
