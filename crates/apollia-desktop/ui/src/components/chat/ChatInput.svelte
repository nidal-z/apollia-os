<!--
  Enriched chat input.

  Responsibilities:
  - Multi-line textarea with auto-resize (1 → 12 lines, scroll beyond).
  - Keyboard: Enter = send, Shift+Enter = newline, Cmd/Ctrl+Enter = force send.
  - Slash-command autocomplete (`/export`, `/rename`).
  - Attachments: paperclip button, drag & drop, preview chips above input.
  - ↑ on empty input re-opens the last user message for editing.
  - Client-side rate-limit (1 send / 500 ms, 30 / min) with soft feedback.
  - Re-render isolation - parent passes props, input state stays local.
-->
<script lang="ts">
  import { onMount, untrack, tick } from "svelte";
  import { t } from "svelte-i18n";
  import {
    InputRewriter,
    fetchWorkContext,
    type RewriteFallback,
  } from "$lib/chat/rewriteInput";
  import { addToast } from "$lib/components/ui/toast";
  import { chatInputAppend } from "$lib/stores/artifacts";
  import type { PendingAttachment } from "$lib/chat/attachments";
  import { createComposerAttachments } from "./useComposerAttachments.svelte";
  import { createComposerDictation } from "./useComposerDictation.svelte";
  import { createComposerRateLimit } from "./useComposerRateLimit.svelte";
  import {
    type SlashCommand,
    detectSlashPrefix,
    filterCommands,
  } from "$lib/chat/slashCommands";
  import {
    type McpResourceView,
    type PinnedResource,
    detectMentionQuery,
    filterResources,
    fetchMcpResources,
    buildPinnedPrefix,
  } from "$lib/chat/mcpResources";
  import SlashCommandMenu from "./SlashCommandMenu.svelte";
  import MentionResourceMenu from "./MentionResourceMenu.svelte";
  import PinnedResourceChip from "./PinnedResourceChip.svelte";
  import AttachmentChip from "./AttachmentChip.svelte";
  import InputHints from "./InputHints.svelte";
  import ChatComposerToolbar from "./ChatComposerToolbar.svelte";

  interface Props {
    disabled: boolean;
    /** True while a turn is generating - swaps Send for a Stop control. */
    busy?: boolean;
    /** Invoked when the user hits Stop during generation. */
    onstop?: () => void;
    onsend: (content: string, attachments: PendingAttachment[]) => void;
    suggestions?: string[];
    /** Last user message text - used by ↑ to pre-fill editing. */
    lastUserMessage?: string | null;
    /** Invoked when the user picks a slash command. */
    oncommand?: (cmdId: SlashCommand["id"]) => void;
    /**
     * Monotonic counter - when it changes, the input pulls `lastUserMessage`
     * into its textarea (used when the parent wants to trigger edit-last
     * without coupling to input internals).
     */
    editLastTrigger?: number;
    /** Current plan-mode state of the session (drives the composer toggle). */
    planMode?: boolean;
    /** When provided, renders a labeled Plan toggle in the toolbar. */
    onplantoggle?: () => void;
    /** Disables the Plan toggle (e.g. closed session). */
    planDisabled?: boolean;
  }

  let {
    disabled,
    busy = false,
    onstop,
    onsend,
    suggestions,
    lastUserMessage = null,
    oncommand,
    editLastTrigger = 0,
    planMode = false,
    onplantoggle,
    planDisabled = false,
  }: Props = $props();

  const DEFAULT_SUGGESTION_KEYS = [
    "chat.placeholder_suggestions.s1",
    "chat.placeholder_suggestions.s2",
    "chat.placeholder_suggestions.s3",
  ];

  const resolvedSuggestions = $derived(
    suggestions && suggestions.length > 0
      ? suggestions
      : DEFAULT_SUGGESTION_KEYS.map((k) => $t(k)),
  );

  const LINE_HEIGHT_PX = 20; // matches text-sm leading
  const MIN_LINES = 1;
  const MAX_LINES = 12;
  const MAX_HEIGHT_PX = LINE_HEIGHT_PX * MAX_LINES + 16; // + vertical padding

  let value = $state("");
  const dictation = createComposerDictation();
  const attachments = createComposerAttachments();
  const rateLimit = createComposerRateLimit();
  let focused = $state(false);

  // ── Rewrite input ──────────────────────────────────────────────────────────
  // "Improve prompt" button that rewrites terse input via LLM before sending.
  // Uses InputRewriter to preserve original text and prevent iteration drift.
  const rewriter = new InputRewriter();
  let isRewriting = $state(false);
  // Mirrors `rewriter.restoreOriginal() !== null` for the template: the class
  // holds plain fields, so a rune has to carry the "a rewrite happened" fact to
  // the restore button.
  let canRestore = $state(false);

  let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
  let fileInputEl = $state<HTMLInputElement | undefined>(undefined);
  let suggestionIndex = $state(0);
  let placeholderVisible = $state(true);
  let reduceMotion = $state(false);
  let isDesktop = $state(false);

  let slashPrefix = $state<string | null>(null);
  let slashCommands = $state<SlashCommand[]>([]);
  let slashIndex = $state(0);

  // ── MCP resource @-mention picker (user-initiative path) ──────────────────
  // When the user types `@`, a picker lists MCP resources from connected
  // servers. Selecting one PINS it; on send, pinned resources are prepended as
  // an explicit system-prefix block. Nothing is auto-injected.
  let mentionQuery = $state<string | null>(null);
  let mentionLoading = $state(false);
  let mentionIndex = $state(0);
  let allResources = $state<McpResourceView[]>([]);
  let resourcesLoaded = $state(false);
  let pinnedResources = $state<PinnedResource[]>([]);

  const filteredMentionResources = $derived(
    mentionQuery === null ? [] : filterResources(allResources, mentionQuery),
  );

  const shouldRotate = $derived(
    !reduceMotion &&
      value === "" &&
      !focused &&
      attachments.items.length === 0 &&
      resolvedSuggestions.length > 1,
  );

  const currentPlaceholder = $derived(
    resolvedSuggestions[suggestionIndex % resolvedSuggestions.length] ??
      $t("chat.input_placeholder"),
  );

  const canSend = $derived(
    !disabled &&
      (value.trim().length > 0 ||
        attachments.items.length > 0 ||
        pinnedResources.length > 0) &&
      rateLimit.blockedReason === null,
  );

  onMount(() => {
    const unsubscribeAppend = chatInputAppend.subscribe((req) => {
      if (req === null) return;
      const suffix = value.length === 0 || value.endsWith("\n") ? "" : " ";
      value = `${value}${suffix}${req.text}`;
      autoResize();
      textareaEl?.focus();
      chatInputAppend.set(null);
    });

    if (typeof window !== "undefined") {
      const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
      reduceMotion = mq.matches;
      const handler = (e: MediaQueryListEvent) => (reduceMotion = e.matches);
      mq.addEventListener("change", handler);
      isDesktop = !/Mobi|Android/i.test(navigator.userAgent);

      return () => {
        mq.removeEventListener("change", handler);
        unsubscribeAppend();
        for (const att of untrack(() => attachments.items)) {
          if (att.previewUrl) URL.revokeObjectURL(att.previewUrl);
        }
      };
    }

    return () => {
      unsubscribeAppend();
    };
  });

  onMount(() =>
    dictation.start((text) => {
      const suffix = value.length === 0 || value.endsWith("\n") ? "" : " ";
      value = `${value}${suffix}${text}`;
      autoResize();
      textareaEl?.focus();
    }),
  );

  /**
   * The sentence the operator reads when no rewrite happened.
   *
   * Four situations reach here and only the first two mean "you have no engine".
   * Sending all four to `error_no_llm` told a user whose model had just timed
   * out to go configure one they already had.
   */
  function rewriteFallbackMessage(fallback: RewriteFallback): string {
    switch (fallback) {
      case "noRouter":
      case "noBackend":
        return $t("chat.rewrite.error_no_llm");
      case "callFailed":
        return $t("chat.rewrite.error_call_failed");
      case "emptyAnswer":
        return $t("chat.rewrite.error_empty_answer");
    }
  }

  async function handleRewrite(): Promise<void> {
    // GIVEN: rewrite already in progress
    if (isRewriting || value.trim() === "") return;

    // WHEN: rewrite triggered
    isRewriting = true;
    try {
      // THEN: fetch Work context from profile
      const workContext = await fetchWorkContext();

      // THEN: call rewriter
      const outcome = await rewriter.rewrite(value, workContext);

      // WHEN: no rewrite happened, whichever of the four reasons stopped it
      // THEN: name that reason and leave the field as it was
      if (outcome.fallback !== null) {
        addToast(rewriteFallbackMessage(outcome.fallback), "error");
        return;
      }

      // WHEN: rewrite succeeds
      // THEN: update value, restore cursor to end, restore focus
      value = outcome.text;
      canRestore = rewriter.restoreOriginal() !== null;
      await tick();
      if (textareaEl) {
        textareaEl.selectionStart = value.length;
        textareaEl.selectionEnd = value.length;
        textareaEl.focus();
      }
    } catch (err) {
      // WHEN: error occurred
      // THEN: show error message
      const errorMsg = err instanceof Error ? err.message : String(err);
      addToast(
        $t("chat.rewrite.error_generic", { values: { error: errorMsg } }),
        "error",
      );
    } finally {
      isRewriting = false;
    }
  }

  async function handleRestoreOriginal(): Promise<void> {
    // GIVEN: a rewrite has been applied at least once
    const original = rewriter.restoreOriginal();
    if (original === null) return;

    // WHEN: the operator asks for the text they typed
    // THEN: put it back, and leave the rewriter armed so a further rewrite
    // still departs from that same original
    value = original;
    await tick();
    if (textareaEl) {
      textareaEl.selectionStart = value.length;
      textareaEl.selectionEnd = value.length;
      textareaEl.focus();
    }
  }

  $effect(() => {
    if (!shouldRotate) return;
    const interval = window.setInterval(() => {
      placeholderVisible = false;
      window.setTimeout(() => {
        suggestionIndex = (suggestionIndex + 1) % resolvedSuggestions.length;
        placeholderVisible = true;
      }, 300);
    }, 4000);
    return () => window.clearInterval(interval);
  });

  $effect(() => {
    // Reset rewriter when field is cleared
    if (value.trim() === "") {
      rewriter.reset();
      canRestore = false;
    }
  });

  $effect(() => {
    // React to value changes for slash-command detection.
    const prefix = detectSlashPrefix(value);
    if (prefix === null) {
      slashPrefix = null;
      slashCommands = [];
      slashIndex = 0;
    } else {
      slashPrefix = prefix;
      // Read the LOCAL list for the index clamp, never the `slashCommands` state
      // we just wrote: filterCommands returns a fresh array every run, so reading
      // the state here would make the effect depend on its own output and spin
      // into an update-depth overflow that freezes the whole app.
      const cmds = filterCommands(prefix);
      slashCommands = cmds;
      if (slashIndex >= cmds.length) slashIndex = 0;
    }
  });

  $effect(() => {
    // React to value changes for @-mention detection. The cursor position is
    // read from the live textarea (not a reactive dep), so we re-read it each
    // time `value` changes.
    const cursor = textareaEl?.selectionStart ?? value.length;
    const query = detectMentionQuery(value, cursor);
    if (query === null) {
      mentionQuery = null;
      mentionIndex = 0;
      return;
    }
    mentionQuery = query;
    // Do NOT read `filteredMentionResources` here: it derives from `mentionQuery`,
    // which this effect writes, so reading it would couple the effect to its own
    // output and can spin into an update-depth overflow that freezes the app.
    mentionIndex = 0;
    if (!resourcesLoaded) {
      void loadResources();
    }
  });

  async function loadResources(): Promise<void> {
    if (resourcesLoaded || mentionLoading) return;
    mentionLoading = true;
    try {
      allResources = await fetchMcpResources();
    } finally {
      resourcesLoaded = true;
      mentionLoading = false;
    }
  }

  function pinResource(resource: McpResourceView): void {
    const exists = pinnedResources.some(
      (p) => p.server === resource.server && p.uri === resource.uri,
    );
    if (!exists) {
      pinnedResources = [
        ...pinnedResources,
        { server: resource.server, uri: resource.uri, name: resource.name },
      ];
    }
    // Strip the in-progress `@token` from the textarea.
    const cursor = textareaEl?.selectionStart ?? value.length;
    const upto = value.slice(0, cursor);
    const at = upto.lastIndexOf("@");
    if (at >= 0) {
      value = value.slice(0, at) + value.slice(cursor);
    }
    mentionQuery = null;
    mentionIndex = 0;
    queueMicrotask(() => {
      autoResize();
      textareaEl?.focus();
    });
  }

  function unpinResource(server: string, uri: string): void {
    pinnedResources = pinnedResources.filter(
      (p) => !(p.server === server && p.uri === uri),
    );
  }

  // Lets a toolbar button (or an outside click) hide an open menu without
  // touching `value`. Read only in the template, so it can never loop.
  let triggerSuppressed = $state(false);
  let inputCardEl = $state<HTMLDivElement | undefined>(undefined);

  // Close an open slash / mention menu when the user clicks outside the composer.
  // The body writes no reactive state (the listener fires on a real click, not
  // during the effect run), so this stays loop-free.
  $effect(() => {
    if (slashPrefix === null && mentionQuery === null) return;
    const onDocMouseDown = (e: MouseEvent): void => {
      if (inputCardEl && !inputCardEl.contains(e.target as Node)) {
        triggerSuppressed = true;
      }
    };
    document.addEventListener("mousedown", onDocMouseDown, true);
    return () => document.removeEventListener("mousedown", onDocMouseDown, true);
  });

  /** Insert a "/" or "@" trigger and focus so the matching menu opens. After the
   *  DOM settles (tick) the caret is at the end; for "@" the value-based detection
   *  ran with a stale caret, so we open the mention menu directly. */
  async function insertTrigger(char: "/" | "@"): Promise<void> {
    const needsSep = value.length > 0 && !/\s$/.test(value);
    value = value + (needsSep ? " " : "") + char;
    await tick();
    autoResize();
    textareaEl?.focus();
    textareaEl?.setSelectionRange(value.length, value.length);
    if (char === "@") {
      mentionQuery = "";
      if (!resourcesLoaded) void loadResources();
    }
  }

  /** Toolbar button: if the trigger's menu is already in play, flip its
   *  visibility; otherwise insert the trigger to open it. */
  function toggleTrigger(char: "/" | "@"): void {
    const present = char === "/" ? slashPrefix !== null : mentionQuery !== null;
    if (present) {
      triggerSuppressed = !triggerSuppressed;
      textareaEl?.focus();
      return;
    }
    triggerSuppressed = false;
    void insertTrigger(char);
  }

  function autoResize() {
    // Typing re-enables a menu the user had hidden via its toolbar button.
    triggerSuppressed = false;
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    const next = Math.min(
      Math.max(textareaEl.scrollHeight, LINE_HEIGHT_PX * MIN_LINES),
      MAX_HEIGHT_PX,
    );
    textareaEl.style.height = `${next}px`;
    textareaEl.style.overflowY = textareaEl.scrollHeight > MAX_HEIGHT_PX ? "auto" : "hidden";
  }

  function handleKeydown(event: KeyboardEvent) {
    // @-mention picker navigation wins when it's open with at least one entry.
    if (mentionQuery !== null && filteredMentionResources.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        mentionIndex = (mentionIndex + 1) % filteredMentionResources.length;
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        mentionIndex =
          (mentionIndex - 1 + filteredMentionResources.length) %
          filteredMentionResources.length;
        return;
      }
      if (event.key === "Tab" || (event.key === "Enter" && !event.shiftKey)) {
        event.preventDefault();
        const resource = filteredMentionResources[mentionIndex];
        if (resource) pinResource(resource);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        mentionQuery = null;
        mentionIndex = 0;
        return;
      }
    }

    // Slash-menu navigation wins when it's open.
    if (slashPrefix !== null && slashCommands.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        slashIndex = (slashIndex + 1) % slashCommands.length;
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        slashIndex = (slashIndex - 1 + slashCommands.length) % slashCommands.length;
        return;
      }
      if (event.key === "Tab" || (event.key === "Enter" && !event.shiftKey)) {
        event.preventDefault();
        const cmd = slashCommands[slashIndex];
        if (cmd) runCommand(cmd);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        slashPrefix = null;
        slashCommands = [];
        return;
      }
    }

    // Cmd/Ctrl+Enter = force send (even mid-line).
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      send();
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
      return;
    }
    // ↑ on empty input → edit last user message.
    if (
      event.key === "ArrowUp" &&
      value === "" &&
      attachments.items.length === 0 &&
      lastUserMessage
    ) {
      event.preventDefault();
      value = lastUserMessage;
      queueMicrotask(() => {
        autoResize();
        textareaEl?.setSelectionRange(value.length, value.length);
      });
      return;
    }
  }

  function send() {
    if (disabled) return;
    const trimmed = value.trim();
    if (!trimmed && attachments.items.length === 0 && pinnedResources.length === 0)
      return;

    if (!rateLimit.admitSend()) return;

    // Prepend the pinned MCP resources as an explicit system-prefix block. This
    // is the user-initiative path: the user chose these, so they ride along
    // this single turn, then the pin list is cleared.
    const prefix = buildPinnedPrefix(pinnedResources);
    const content = prefix ? `${prefix}${trimmed}` : trimmed;

    onsend(content, attachments.takeAll());
    value = "";
    pinnedResources = [];
    if (textareaEl) textareaEl.style.height = "auto";
  }

  function runCommand(cmd: SlashCommand) {
    // Remove the slash token from the input, keep any text after.
    value = value.replace(/^\s*\/[^\s\n]*\s?/, "");
    slashPrefix = null;
    slashCommands = [];
    oncommand?.(cmd.id);
  }

  $effect(() => {
    if (!disabled && textareaEl) textareaEl.focus();
  });

  $effect(() => {
    // Trigger programmatic edit-last (parent increments `editLastTrigger`).
    if (editLastTrigger > 0 && lastUserMessage) {
      untrack(() => {
        value = lastUserMessage;
        queueMicrotask(() => {
          autoResize();
          textareaEl?.focus();
          textareaEl?.setSelectionRange(value.length, value.length);
        });
      });
    }
  });
</script>

<div class="border-t border-border/30 px-4 pb-2 pt-2" data-testid="chat-input">
  <input
    bind:this={fileInputEl}
    type="file"
    multiple
    class="hidden"
    onchange={attachments.handleFileInput}
    data-testid="chat-attach-input"
  />

  <div
    bind:this={inputCardEl}
    class="relative flex flex-col rounded-xl border bg-surface-1 transition-colors {focused ? 'border-primary/60' : 'border-border/60'}"
    class:ring-2={attachments.dragOver}
    class:ring-primary={attachments.dragOver}
    ondragover={attachments.handleDragOver}
    ondragleave={attachments.handleDragLeave}
    ondrop={attachments.handleDrop}
    role="presentation"
  >
    {#if slashPrefix !== null && !triggerSuppressed}
      <SlashCommandMenu
        commands={slashCommands}
        selectedIndex={slashIndex}
        onselect={(cmd) => runCommand(cmd)}
        onhover={(i) => (slashIndex = i)}
      />
    {/if}

    {#if mentionQuery !== null && !triggerSuppressed}
      <MentionResourceMenu
        resources={filteredMentionResources}
        selectedIndex={mentionIndex}
        loading={mentionLoading && !resourcesLoaded}
        onselect={(resource) => pinResource(resource)}
        onhover={(i) => (mentionIndex = i)}
      />
    {/if}

    {#if pinnedResources.length > 0 || attachments.items.length > 0}
      <div class="flex flex-wrap gap-1.5 px-3 pt-2.5" data-testid="chat-chip-row">
        {#each pinnedResources as pin (pin.server + "::" + pin.uri)}
          <PinnedResourceChip
            resource={pin}
            onremove={() => unpinResource(pin.server, pin.uri)}
          />
        {/each}
        {#each attachments.items as att (att.id)}
          <AttachmentChip
            attachment={att}
            onremove={() => attachments.remove(att.id)}
          />
        {/each}
      </div>
    {/if}

    <textarea
      bind:this={textareaEl}
      bind:value
      oninput={autoResize}
      onkeydown={handleKeydown}
      onfocus={() => (focused = true)}
      onblur={() => (focused = false)}
      {disabled}
      rows="1"
      placeholder={currentPlaceholder}
      aria-label={$t("chat.input_placeholder")}
      class="chat-input-textarea block w-full resize-none bg-transparent px-3.5 pb-1 pt-3 text-sm text-foreground
        outline-none placeholder:text-muted-foreground/40
        disabled:cursor-not-allowed disabled:opacity-50"
      class:placeholder-fading={!placeholderVisible}
    ></textarea>

    <ChatComposerToolbar
      {disabled}
      {busy}
      {isDesktop}
      {canSend}
      hasText={value.trim() !== ""}
      {planMode}
      {planDisabled}
      {onplantoggle}
      {dictation}
      {isRewriting}
      {canRestore}
      onattach={() => fileInputEl?.click()}
      onrewrite={handleRewrite}
      onrestore={handleRestoreOriginal}
      ontrigger={toggleTrigger}
      {onstop}
      onsend={send}
    />
  </div>

  <InputHints
    status={dictation.error ?? rateLimit.status}
    statusTone={dictation.error ? "warn" : rateLimit.tone}
  />
</div>

<style>
  .chat-input-textarea::placeholder {
    transition: opacity var(--motion-base) ease;
    opacity: 1;
  }
  .chat-input-textarea.placeholder-fading::placeholder {
    opacity: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .chat-input-textarea::placeholder {
      transition: none;
    }
  }
</style>
