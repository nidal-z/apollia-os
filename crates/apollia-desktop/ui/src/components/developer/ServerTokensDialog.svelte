<script lang="ts">
  /**
   * "Manage tokens": the window of the tokens of the local server. It lists
   * the tokens in use (name, last four characters, creation, last use),
   * creates a named one and revokes one after a confirmation.
   *
   * The value of a token is shown once, right after its creation, with a copy
   * button and the warning that it will not be shown again. It lives in the
   * state of this component and nowhere else: closing the window drops it.
   */
  import { locale, t } from "svelte-i18n";
  import { Check, Copy, KeyRound, TriangleAlert } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    createServerToken,
    listServerTokens,
    revokeServerToken,
    type ServerToken,
  } from "$lib/ipc/serverTokens";
  import { formatRelativeTime, parseTimestampMs } from "$lib/utils";
  import {
    CLOSED_WINDOW,
    LONGEST_TOKEN_NAME,
    SERVER_TOKENS_TEXTS,
    activeTokenCount,
    closeTokens,
    copyShownToken,
    createToken,
    openTokens,
    revokeToken,
    tokensInUse,
    type ServerTokensIpc,
    type TokensWindow,
  } from "./serverTokens";

  interface Props {
    open: boolean;
    onclose: () => void;
    /** The list was read or changed: how many tokens are in use. */
    oncount: (count: number) => void;
  }

  let { open, onclose, oncount }: Props = $props();

  const ipc: ServerTokensIpc = {
    list: listServerTokens,
    create: createServerToken,
    revoke: revokeServerToken,
  };

  let current = $state<TokensWindow>(CLOSED_WINDOW);
  let name = $state("");
  let creating = $state(false);
  let revoking = $state(false);
  let asked = $state<ServerToken | null>(null);
  let copied = $state(false);

  const listed = $derived(tokensInUse(current));
  const failure = $derived(current.failure);
  const language = $derived($locale ?? "en");

  function take(next: TokensWindow): void {
    current = next;
    const count = activeTokenCount(next);
    if (count !== null) oncount(count);
  }

  $effect(() => {
    if (!open) {
      // Nothing of a closed window is kept: the value shown, the name typed.
      current = closeTokens();
      name = "";
      asked = null;
      copied = false;
      return;
    }
    let live = true;
    void openTokens(ipc).then((read) => {
      if (live) take(read);
    });
    return () => {
      live = false;
    };
  });

  async function create(): Promise<void> {
    if (creating) return;
    creating = true;
    copied = false;
    const next = await createToken(ipc, current, name);
    // A window closed while the runtime answered takes nothing back.
    if (open) {
      take(next);
      if (next.failure === null) name = "";
    }
    creating = false;
  }

  async function revoke(): Promise<void> {
    if (asked === null || revoking) return;
    revoking = true;
    const next = await revokeToken(ipc, current, asked.id);
    if (open) take(next);
    asked = null;
    revoking = false;
  }

  async function copy(): Promise<void> {
    const done = await copyShownToken(current, (text) => navigator.clipboard.writeText(text));
    if (!done) {
      addToast($t(SERVER_TOKENS_TEXTS.copyFailed), "error");
      return;
    }
    copied = true;
  }

  function createdOn(token: ServerToken): string {
    const at = parseTimestampMs(token.createdAt);
    if (Number.isNaN(at)) return token.createdAt;
    return new Date(at).toLocaleDateString(language, {
      day: "numeric",
      month: "short",
      year: "numeric",
    });
  }

  function onEnter(event: KeyboardEvent): void {
    if (event.key === "Enter") void create();
  }
</script>

{#if $developerMode}
  <Dialog
    {open}
    {onclose}
    size="md"
    title={$t(SERVER_TOKENS_TEXTS.title)}
    data-testid="developer-server-tokens"
  >
    <div class="flex flex-col gap-4">
      <p class="text-sm text-muted-foreground">{$t(SERVER_TOKENS_TEXTS.intro)}</p>

      {#if current.shown}
        <div
          class="flex flex-col gap-2 rounded-lg bg-muted p-3"
          data-testid="developer-server-token-shown"
        >
          <p class="text-sm font-medium text-foreground">
            {$t(SERVER_TOKENS_TEXTS.createdTitle, { values: { name: current.shown.name } })}
          </p>
          <div
            class="flex items-center gap-2 rounded-md border border-border bg-background px-2 py-1"
          >
            <code
              class="min-w-0 flex-1 break-all font-mono text-xs text-foreground"
              data-testid="developer-server-token-value">{current.shown.value}</code
            >
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={$t(copied ? SERVER_TOKENS_TEXTS.copied : SERVER_TOKENS_TEXTS.copy)}
              onclick={() => void copy()}
              data-testid="developer-server-token-copy"
            >
              {#if copied}
                <Check size={14} aria-hidden="true" />
              {:else}
                <Copy size={14} aria-hidden="true" />
              {/if}
            </Button>
          </div>
          <p
            class="flex items-start gap-2 text-xs text-warning-a11y"
            data-testid="developer-server-token-shown-once"
          >
            <TriangleAlert class="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
            <span>{$t(SERVER_TOKENS_TEXTS.shownOnce)}</span>
          </p>
        </div>
      {/if}

      {#if failure?.kind === "read"}
        <p class="text-sm text-destructive" role="alert" data-testid="developer-server-tokens-unread">
          {$t(SERVER_TOKENS_TEXTS.readFailed, { values: { error: failure.message } })}
        </p>
      {:else if listed.length === 0}
        <p class="text-sm text-muted-foreground" data-testid="developer-server-tokens-empty">
          {$t(SERVER_TOKENS_TEXTS.empty)}
        </p>
      {:else}
        <ul
          class="flex flex-col divide-y divide-border rounded-lg border border-border"
          data-testid="developer-server-tokens-list"
        >
          {#each listed as token (token.id)}
            <li class="flex items-center gap-3 px-3 py-2" data-testid={`developer-server-token-row-${token.id}`}>
              <KeyRound class="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden="true" />
              <div class="flex min-w-0 flex-1 flex-col">
                <span class="flex items-baseline gap-2">
                  <span class="truncate text-sm font-medium text-foreground">{token.name}</span>
                  <span class="font-mono text-xs text-muted-foreground">…{token.lastFour}</span>
                </span>
                <span class="text-xs text-muted-foreground">
                  {$t(SERVER_TOKENS_TEXTS.createdOn, { values: { date: createdOn(token) } })}
                  ·
                  {#if token.lastUsedAt}
                    {$t(SERVER_TOKENS_TEXTS.lastUsed, {
                      values: { when: formatRelativeTime(token.lastUsedAt, language) },
                    })}
                  {:else}
                    {$t(SERVER_TOKENS_TEXTS.neverUsed)}
                  {/if}
                </span>
              </div>
              <Button
                variant="outline"
                size="sm"
                aria-label={$t(SERVER_TOKENS_TEXTS.revokeLabel, { values: { name: token.name } })}
                onclick={() => (asked = token)}
                data-testid={`developer-server-token-row-${token.id}-revoke`}
              >
                {$t(SERVER_TOKENS_TEXTS.revoke)}
              </Button>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="flex flex-col gap-1">
        <div class="flex items-center gap-2">
          <Input
            class="flex-1"
            bind:value={name}
            maxlength={LONGEST_TOKEN_NAME}
            autocomplete="off"
            spellcheck={false}
            placeholder={$t(SERVER_TOKENS_TEXTS.namePlaceholder)}
            aria-label={$t(SERVER_TOKENS_TEXTS.nameLabel)}
            aria-invalid={failure?.kind === "name_empty" ||
              failure?.kind === "name_invalid" ||
              failure?.kind === "name_taken"}
            onkeydown={onEnter}
            data-testid="developer-server-token-name"
          />
          <Button loading={creating} onclick={() => void create()} data-testid="developer-server-token-create">
            {$t(SERVER_TOKENS_TEXTS.create)}
          </Button>
        </div>
        {#if failure && failure.kind !== "read"}
          <p
            class="text-xs text-destructive"
            role="alert"
            data-testid={`developer-server-tokens-error-${failure.kind}`}
          >
            {#if failure.kind === "name_empty"}
              {$t(SERVER_TOKENS_TEXTS.nameEmpty)}
            {:else if failure.kind === "name_invalid"}
              {$t(SERVER_TOKENS_TEXTS.nameInvalid)}
            {:else if failure.kind === "name_taken"}
              {$t(SERVER_TOKENS_TEXTS.nameTaken, { values: { name: failure.name } })}
            {:else if failure.kind === "create"}
              {$t(SERVER_TOKENS_TEXTS.createFailed, { values: { error: failure.message } })}
            {:else}
              {$t(SERVER_TOKENS_TEXTS.revokeFailed, { values: { error: failure.message } })}
            {/if}
          </p>
        {/if}
      </div>
    </div>

    <DialogFooter>
      <Button variant="outline" onclick={onclose} data-testid="developer-server-tokens-close">
        {$t(SERVER_TOKENS_TEXTS.close)}
      </Button>
    </DialogFooter>
  </Dialog>

  <ConfirmDialog
    open={open && asked !== null}
    onclose={() => (asked = null)}
    onconfirm={() => void revoke()}
    title={$t(SERVER_TOKENS_TEXTS.revokeTitle, { values: { name: asked?.name ?? "" } })}
    message={$t(SERVER_TOKENS_TEXTS.revokeMessage, { values: { lastFour: asked?.lastFour ?? "" } })}
    confirmLabel={$t(SERVER_TOKENS_TEXTS.revoke)}
    loading={revoking}
    data-testid="developer-server-token-revoke"
  />
{/if}
