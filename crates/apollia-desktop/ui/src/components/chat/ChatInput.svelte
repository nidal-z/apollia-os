<!--
  Enriched chat input (US-SP42-026).

  Responsibilities:
  - Multi-line textarea with auto-resize (1 → 12 lines, scroll beyond).
  - Keyboard: Enter = send, Shift+Enter = newline, Cmd/Ctrl+Enter = force send.
  - Slash-command autocomplete (`/clear`, `/export`, `/rename`, `/memory`, `/tools`).
  - Attachments: paperclip button, drag & drop, preview chips above input.
  - ↑ on empty input re-opens the last user message for editing (B.42).
  - Client-side rate-limit (1 send / 500 ms, 30 / min) with soft feedback (B.67).
  - Re-render isolation — parent passes props, input state stays local (B.66).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { Send, Paperclip } from "lucide-svelte";
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
  import SlashCommandMenu from "./SlashCommandMenu.svelte";
  import AttachmentChip from "./AttachmentChip.svelte";
  import InputHints from "./InputHints.svelte";

  interface Props {
    disabled: boolean;
    onsend: (content: string, attachments: PendingAttachment[]) => void;
    suggestions?: string[];
    /** Last user message text — used by ↑ to pre-fill editing (B.42). */
    lastUserMessage?: string | null;
    /** Invoked when the user picks a slash command. */
    oncommand?: (cmdId: SlashCommand["id"]) => void;
    /**
     * Monotonic counter — when it changes, the input pulls `lastUserMessage`
     * into its textarea (used when the parent wants to trigger edit-last
     * without coupling to input internals).
     */
    editLastTrigger?: number;
  }

  let {
    disabled,
    onsend,
    suggestions,
    lastUserMessage = null,
    oncommand,
    editLastTrigger = 0,
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

  const limiter = new ChatRateLimiter();

  // US-SP42-035 (B.67): reactive rate-limit state so the Send button can be
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
            default: "Rate limit reached — retry in {s}s",
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
      (value.trim().length > 0 || attachments.length > 0) &&
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
    // ↑ on empty input → edit last user message (B.42).
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
    if (!trimmed && attachments.length === 0) return;

    const check = limiter.check();
    if (!check.allowed) {
      refreshRateState();
      ensureRateBlockTimer();
      return;
    }
    limiter.record();
    // After a successful send, the min-interval cooldown kicks in — surface it.
    refreshRateState();
    ensureRateBlockTimer();

    const payload = attachments;
    onsend(trimmed, payload);
    value = "";
    attachments = [];
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
  {#if attachments.length > 0}
    <div
      class="mb-2 flex flex-wrap gap-1.5"
      data-testid="chat-attachment-list"
    >
      {#each attachments as att (att.id)}
        <AttachmentChip
          attachment={att}
          onremove={() => removeAttachment(att.id)}
        />
      {/each}
    </div>
  {/if}

  <div
    class="relative flex items-end gap-2 rounded-lg border border-border/40 bg-card/50 transition-colors"
    class:ring-2={dragOver}
    class:ring-primary={dragOver}
    class:border-primary={focused}
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

    <button
      type="button"
      onclick={handlePaperclip}
      disabled={disabled}
      class="mb-1.5 ml-1.5 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground/60 hover:bg-muted/40 hover:text-foreground disabled:opacity-30"
      aria-label={$t("chat.attachments.add")}
      data-testid="chat-attach-button"
    >
      <Paperclip size={14} />
    </button>
    <input
      bind:this={fileInputEl}
      type="file"
      multiple
      class="hidden"
      onchange={handleFileInput}
      data-testid="chat-attach-input"
    />

    <div class="relative flex-1">
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
        class="chat-input-textarea block w-full resize-none bg-transparent px-1 py-2 text-sm text-foreground
          outline-none placeholder:text-muted-foreground/40
          disabled:cursor-not-allowed disabled:opacity-50"
        class:placeholder-fading={!placeholderVisible}
      ></textarea>
      {#if isDesktop && value === "" && !focused && attachments.length === 0}
        <span
          class="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 select-none text-[10px] text-muted-foreground/40"
          aria-hidden="true"
        >
          ⌘/
        </span>
      {/if}
    </div>

    <button
      onclick={send}
      disabled={!canSend}
      class="mb-1.5 mr-1.5 inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md
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
      <Send size={14} />
    </button>
  </div>

  <InputHints status={rateStatus} statusTone={rateTone} />
</div>

<style>
  .chat-input-textarea::placeholder {
    transition: opacity 300ms ease;
    opacity: 1;
  }
  .chat-input-textarea.placeholder-fading::placeholder {
    opacity: 0;
  }
  :global(.shadow-warm-glow) {
    box-shadow: 0 0 0 1px rgba(255, 180, 120, 0.35), 0 4px 14px -6px rgba(255, 150, 80, 0.45);
  }
  @media (prefers-reduced-motion: reduce) {
    .chat-input-textarea::placeholder {
      transition: none;
    }
  }
</style>
