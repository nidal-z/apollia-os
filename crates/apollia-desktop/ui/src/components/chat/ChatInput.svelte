<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Send } from "lucide-svelte";
  import { tourPrefill } from "$lib/stores/tour";

  interface Props {
    disabled: boolean;
    onsend: (content: string) => void;
    suggestions?: string[];
  }

  let { disabled, onsend, suggestions }: Props = $props();

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

  let value = $state("");
  let focused = $state(false);
  let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
  let suggestionIndex = $state(0);
  let placeholderVisible = $state(true);
  let reduceMotion = $state(false);
  let isDesktop = $state(false);

  const shouldRotate = $derived(
    !reduceMotion && value === "" && !focused && resolvedSuggestions.length > 1,
  );

  const currentPlaceholder = $derived(
    resolvedSuggestions[suggestionIndex % resolvedSuggestions.length] ??
      $t("chat.input_placeholder"),
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

    if (typeof window !== "undefined") {
      const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
      reduceMotion = mq.matches;
      const handler = (e: MediaQueryListEvent) => (reduceMotion = e.matches);
      mq.addEventListener("change", handler);
      // Heuristic: Tauri desktop build sets navigator.platform / userAgent.
      isDesktop = !/Mobi|Android/i.test(navigator.userAgent);

      return () => {
        mq.removeEventListener("change", handler);
        unsubscribe();
      };
    }

    return unsubscribe;
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

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 160) + "px";
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  }

  function send() {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onsend(trimmed);
    value = "";
    if (textareaEl) textareaEl.style.height = "auto";
  }

  $effect(() => {
    if (!disabled && textareaEl) textareaEl.focus();
  });
</script>

<div class="border-t border-border/30 px-4 pb-3 pt-2" data-testid="chat-input">
  <div class="relative flex items-end gap-2">
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
        class="chat-input-textarea w-full resize-none rounded-lg border border-border/40 bg-card/50 px-3 py-2 text-sm text-foreground
          outline-none transition-shadow placeholder:text-muted-foreground/40
          focus:ring-1 focus:ring-primary/20 focus:border-primary/30
          disabled:cursor-not-allowed disabled:opacity-50"
        class:placeholder-fading={!placeholderVisible}
      ></textarea>
      {#if isDesktop && value === "" && !focused}
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
      disabled={disabled || !value.trim()}
      class="bg-primary-solid inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg
        active:scale-[0.96]
        disabled:pointer-events-none disabled:opacity-30"
      aria-label={$t("chat.send")}
      data-testid="chat-send-button"
    >
      <Send size={15} />
    </button>
  </div>
</div>

<style>
  .chat-input-textarea::placeholder {
    transition: opacity 300ms ease;
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
