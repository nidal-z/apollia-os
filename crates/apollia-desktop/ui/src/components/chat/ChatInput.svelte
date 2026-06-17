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
  import { Send, Paperclip, Mic, MicOff, Slash, AtSign, ListChecks } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { tourPrefill } from "$lib/stores/tour";
  import { chatInputAppend } from "$lib/stores/artifacts";
  import { ChatRateLimiter } from "$lib/chat/rateLimit";
  import {
    type PendingAttachment,
    attachmentId,
    classifyKind,
    readAsBase64,
    INLINE_MAX_BYTES,
  } from "$lib/chat/attachments";
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

  interface Props {
    disabled: boolean;
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
  // ── Speech-to-text - bouton micro ───────────────────────────────────────
  // Le bouton mic toggle l'enregistrement en réutilisant les Tauri commands
  // existants `start_tour_recording` / `stop_tour_recording`. La transcription
  // arrive via l'event Tauri `stt-transcribed` (broadcast par le pipeline
  // Whisper) - on l'insère dans le textarea à ce moment-là.
  let recording = $state(false);
  let sttBusy = $state(false);
  let sttUnlisten: UnlistenFn | null = null;
  let focused = $state(false);
  let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
  let fileInputEl = $state<HTMLInputElement | undefined>(undefined);
  let suggestionIndex = $state(0);
  let placeholderVisible = $state(true);
  let reduceMotion = $state(false);
  let isDesktop = $state(false);
  let dragOver = $state(false);

  let attachments = $state<PendingAttachment[]>([]);
  let rateStatus = $state<string | null>(null);
  let rateTone = $state<"neutral" | "warn">("neutral");

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

  const limiter = new ChatRateLimiter();

  // reactive rate-limit state so the Send button can be
  // pre-disabled and a visible countdown is shown until the cooldown elapses.
  let rateBlockedMs = $state<number>(0);
  let rateBlockedReason = $state<"too_fast" | "too_many" | null>(null);
  let rateBlockTimer: ReturnType<typeof setInterval> | undefined;

  function refreshRateState(): void {
    const check = limiter.check();
    if (check.allowed) {
      rateBlockedMs = 0;
      rateBlockedReason = null;
      rateStatus = null;
      rateTone = "neutral";
    } else {
      rateBlockedMs = check.retryAfterMs ?? 0;
      rateBlockedReason = check.reason ?? null;
      rateStatus = check.reason === "too_fast"
        ? $t("chat.rate_limit.too_fast_countdown", {
            default: "Wait {s}s before sending again",
            values: { s: Math.ceil(rateBlockedMs / 1000) },
          })
        : $t("chat.rate_limit.too_many_countdown", {
            default: "Rate limit reached - retry in {s}s",
            values: { s: Math.ceil(rateBlockedMs / 1000) },
          });
      rateTone = "warn";
    }
  }

  function ensureRateBlockTimer(): void {
    if (rateBlockTimer !== undefined) return;
    rateBlockTimer = setInterval(() => {
      refreshRateState();
      if (rateBlockedReason === null && rateBlockTimer !== undefined) {
        clearInterval(rateBlockTimer);
        rateBlockTimer = undefined;
      }
    }, 200);
  }

  const shouldRotate = $derived(
    !reduceMotion &&
      value === "" &&
      !focused &&
      attachments.length === 0 &&
      resolvedSuggestions.length > 1,
  );

  const currentPlaceholder = $derived(
    resolvedSuggestions[suggestionIndex % resolvedSuggestions.length] ??
      $t("chat.input_placeholder"),
  );

  const canSend = $derived(
    !disabled &&
      (value.trim().length > 0 ||
        attachments.length > 0 ||
        pinnedResources.length > 0) &&
      rateBlockedReason === null,
  );

  onMount(() => {
    const unsubscribe = tourPrefill.subscribe((interaction) => {
      if (
        interaction !== null &&
        interaction.interaction_type === "send_chat" &&
        interaction.prefilled_data !== null &&
        interaction.prefilled_data !== undefined
      ) {
        const msg = interaction.prefilled_data["message"];
        if (typeof msg === "string" && value === "") {
          value = msg;
          autoResize();
        }
      }
    });

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
        unsubscribe();
        unsubscribeAppend();
        for (const att of untrack(() => attachments)) {
          if (att.previewUrl) URL.revokeObjectURL(att.previewUrl);
        }
      };
    }

    return () => {
      unsubscribe();
      unsubscribeAppend();
    };
  });

  // STT event listener: when the user records (via the mic button OR via
  // the global hotkey), the Rust pipeline broadcasts `stt-transcribed`
  // with the resulting text. We append it to the input textarea.
  onMount(() => {
    let cancelled = false;
    void listen<{ text?: string } | string>("stt-transcribed", (event) => {
      const text =
        typeof event.payload === "string"
          ? event.payload
          : event.payload?.text ?? "";
      if (!text) return;
      const suffix = value.length === 0 || value.endsWith("\n") ? "" : " ";
      value = `${value}${suffix}${text}`;
      recording = false;
      sttBusy = false;
      autoResize();
      textareaEl?.focus();
    }).then((unlisten) => {
      if (cancelled) {
        unlisten();
      } else {
        sttUnlisten = unlisten;
      }
    });
    return () => {
      cancelled = true;
      sttUnlisten?.();
      sttUnlisten = null;
    };
  });

  async function toggleMic(): Promise<void> {
    if (sttBusy) return;
    sttBusy = true;
    try {
      if (recording) {
        await invoke("stop_tour_recording");
        // The transcription event will flip recording=false once it arrives.
        // We keep sttBusy=true until the event listener clears it, so the
        // user can't double-click during the inference.
      } else {
        await invoke("start_tour_recording");
        recording = true;
        sttBusy = false;
      }
    } catch {
      // STT engine unavailable, no model configured, etc. - surface nothing
      // here (the global hotkey listener already toasts) and reset state.
      recording = false;
      sttBusy = false;
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
    // React to value changes for slash-command detection.
    const prefix = detectSlashPrefix(value);
    if (prefix === null) {
      slashPrefix = null;
      slashCommands = [];
      slashIndex = 0;
    } else {
      slashPrefix = prefix;
      slashCommands = filterCommands(prefix);
      if (slashIndex >= slashCommands.length) slashIndex = 0;
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

  // Shared styling for the composer toolbar icon buttons (single source so the
  // gutter no longer mixes h-7 / h-8 and Button-vs-button treatments).
  const toolBtn =
    "inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40 disabled:cursor-not-allowed";

  /** Insert a "/" or "@" trigger and focus so the matching menu opens. After the
   *  DOM settles (tick) the caret is at the end; for "@" the value-based detection
   *  ran with a stale caret, so we open the mention menu directly. No reactive
   *  plumbing here, so this can never feed an effect loop. */
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

  function autoResize() {
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
      attachments.length === 0 &&
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
    if (!trimmed && attachments.length === 0 && pinnedResources.length === 0)
      return;

    const check = limiter.check();
    if (!check.allowed) {
      refreshRateState();
      ensureRateBlockTimer();
      return;
    }
    limiter.record();
    // After a successful send, the min-interval cooldown kicks in - surface it.
    refreshRateState();
    ensureRateBlockTimer();

    // Prepend the pinned MCP resources as an explicit system-prefix block. This
    // is the user-initiative path: the user chose these, so they ride along
    // this single turn, then the pin list is cleared.
    const prefix = buildPinnedPrefix(pinnedResources);
    const content = prefix ? `${prefix}${trimmed}` : trimmed;

    const payload = attachments;
    onsend(content, payload);
    value = "";
    attachments = [];
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

  async function ingestFiles(files: FileList | File[]): Promise<void> {
    const list = Array.from(files);
    for (const file of list) {
      const kind = classifyKind(file.type, file.name);
      const att: PendingAttachment = {
        id: attachmentId(),
        name: file.name,
        mime: file.type || "application/octet-stream",
        size: file.size,
        kind,
      };
      if (kind === "image") {
        att.previewUrl = URL.createObjectURL(file);
      }
      try {
        if (file.size <= INLINE_MAX_BYTES) {
          att.base64 = await readAsBase64(file);
        } else {
          // Desktop drop events expose `path` on the File (Tauri). Fallback to name.
          const anyFile = file as unknown as { path?: string };
          if (anyFile.path) att.absolutePath = anyFile.path;
        }
      } catch (err) {
        console.warn("attachment read failed", err);
      }
      attachments = [...attachments, att];
    }
  }

  function removeAttachment(id: string): void {
    const target = attachments.find((a) => a.id === id);
    if (target?.previewUrl) URL.revokeObjectURL(target.previewUrl);
    attachments = attachments.filter((a) => a.id !== id);
  }

  function handlePaperclip(): void {
    fileInputEl?.click();
  }

  function handleFileInput(event: Event): void {
    const input = event.target as HTMLInputElement;
    if (input.files && input.files.length > 0) {
      void ingestFiles(input.files);
    }
    input.value = "";
  }

  function handleDragOver(event: DragEvent): void {
    event.preventDefault();
    dragOver = true;
  }

  function handleDragLeave(event: DragEvent): void {
    event.preventDefault();
    dragOver = false;
  }

  function handleDrop(event: DragEvent): void {
    event.preventDefault();
    dragOver = false;
    const files = event.dataTransfer?.files;
    if (files && files.length > 0) void ingestFiles(files);
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
    onchange={handleFileInput}
    data-testid="chat-attach-input"
  />

  <div
    class="relative flex flex-col rounded-xl border bg-surface-1 transition-colors {focused ? 'border-primary/60' : 'border-border/60'}"
    class:ring-2={dragOver}
    class:ring-primary={dragOver}
    ondragover={handleDragOver}
    ondragleave={handleDragLeave}
    ondrop={handleDrop}
    role="presentation"
  >
    {#if slashPrefix !== null}
      <SlashCommandMenu
        commands={slashCommands}
        selectedIndex={slashIndex}
        onselect={(cmd) => runCommand(cmd)}
        onhover={(i) => (slashIndex = i)}
      />
    {/if}

    {#if mentionQuery !== null}
      <MentionResourceMenu
        resources={filteredMentionResources}
        selectedIndex={mentionIndex}
        loading={mentionLoading && !resourcesLoaded}
        onselect={(resource) => pinResource(resource)}
        onhover={(i) => (mentionIndex = i)}
      />
    {/if}

    {#if pinnedResources.length > 0 || attachments.length > 0}
      <div class="flex flex-wrap gap-1.5 px-3 pt-2.5" data-testid="chat-chip-row">
        {#each pinnedResources as pin (pin.server + "::" + pin.uri)}
          <PinnedResourceChip
            resource={pin}
            onremove={() => unpinResource(pin.server, pin.uri)}
          />
        {/each}
        {#each attachments as att (att.id)}
          <AttachmentChip
            attachment={att}
            onremove={() => removeAttachment(att.id)}
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

    <!-- Action toolbar: secondary actions left, send anchored right. -->
    <div class="flex items-center gap-0.5 border-t border-border/40 px-2 py-1.5">
      {#if onplantoggle}
        <button
          type="button"
          onclick={onplantoggle}
          disabled={disabled || planDisabled}
          aria-pressed={planMode}
          class="mr-1 inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 {planMode
            ? 'border-primary/40 bg-primary/12 text-primary'
            : 'border-border text-muted-foreground hover:bg-muted/50 hover:text-foreground'}"
          aria-label={$t("chat.planMode.chipLabel")}
          title={$t("chat.planMode.chipLabel")}
          data-testid="composer-plan-toggle"
        >
          <ListChecks size={12} class="shrink-0" />
          {$t("chat.planMode.chipLabel")}
        </button>
      {/if}
      <button
        type="button"
        onclick={handlePaperclip}
        {disabled}
        class={toolBtn}
        aria-label={$t("chat.attachments.add")}
        title={$t("chat.attachments.add")}
        data-testid="chat-attach-button"
      >
        <Paperclip size={16} />
      </button>
      <button
        type="button"
        onclick={toggleMic}
        disabled={disabled || (sttBusy && !recording)}
        class="{toolBtn} {recording ? 'bg-destructive text-destructive-foreground hover:bg-destructive hover:text-destructive-foreground mic-pulse' : ''}"
        aria-label={recording ? $t("chat.dictate_stop") : $t("chat.dictate_start")}
        title={recording ? $t("chat.dictate_stop") : $t("chat.dictate_start")}
        data-testid="chat-mic-button"
      >
        {#if recording}
          <MicOff size={16} />
        {:else}
          <Mic size={16} />
        {/if}
      </button>
      <button
        type="button"
        onclick={() => insertTrigger("/")}
        {disabled}
        class={toolBtn}
        aria-label={$t("chat.slash_commands")}
        title={$t("chat.slash_commands")}
        data-testid="chat-slash-button"
      >
        <Slash size={16} />
      </button>
      <button
        type="button"
        onclick={() => insertTrigger("@")}
        {disabled}
        class={toolBtn}
        aria-label={$t("chat.mention_resources")}
        title={$t("chat.mention_resources")}
        data-testid="chat-mention-button"
      >
        <AtSign size={16} />
      </button>

      <div class="flex-1"></div>

      {#if isDesktop}
        <span class="mr-1.5 select-none text-[11px] text-muted-foreground/50" aria-hidden="true">
          ⌘↵
        </span>
      {/if}
      <button
        onclick={send}
        disabled={!canSend}
        class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md
          transition-all active:scale-[0.96]
          disabled:pointer-events-none disabled:opacity-40 disabled:cursor-not-allowed"
        class:bg-primary-solid={canSend}
        class:text-primary-foreground={canSend}
        class:shadow-warm-glow={canSend}
        class:bg-muted={!canSend}
        class:text-muted-foreground={!canSend}
        aria-label={$t("chat.send")}
        data-testid="chat-send-button"
      >
        <Send size={16} />
      </button>
    </div>
  </div>

  <InputHints status={rateStatus} statusTone={rateTone} />
</div>

<style>
  .chat-input-textarea::placeholder {
    transition: opacity 300ms ease;
    opacity: 1;
  }

  /* Subtle pulse around the mic button while recording. */
  :global(.mic-pulse) {
    box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.5);
    animation: mic-pulse 1.4s ease-out infinite;
  }
  @keyframes mic-pulse {
    0% {
      box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.55);
    }
    70% {
      box-shadow: 0 0 0 6px hsl(var(--destructive) / 0);
    }
    100% {
      box-shadow: 0 0 0 0 hsl(var(--destructive) / 0);
    }
  }
  .chat-input-textarea.placeholder-fading::placeholder {
    opacity: 0;
  }
  :global(.shadow-warm-glow) {
    box-shadow: var(--shadow-warm-focus);
  }
  @media (prefers-reduced-motion: reduce) {
    .chat-input-textarea::placeholder {
      transition: none;
    }
  }
</style>
