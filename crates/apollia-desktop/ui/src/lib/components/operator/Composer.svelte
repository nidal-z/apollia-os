<script lang="ts">
  import {
    Paperclip,
    Slash,
    AtSign,
    Mic,
    ArrowUp,
    File,
    X,
    Sparkles,
  } from "lucide-svelte";
  import Chip from "./Chip.svelte";
  import Kbd from "./Kbd.svelte";

  export type ComposerState = "default" | "typing" | "slash" | "mentions";

  export type Attachment = { name: string; size: string };
  export type SlashCmd = {
    cmd: string;
    desc: string;
    active?: boolean;
  };
  export type Mention = { name: string; description?: string; active?: boolean };

  interface Props {
    value?: string;
    placeholder?: string;
    /** Attachments shown as chips above the input. */
    attachments?: Attachment[];
    /** Slash menu items (only rendered when state === "slash"). */
    slashCommands?: SlashCmd[];
    /** Mention menu items (only rendered when state === "mentions"). */
    mentions?: Mention[];
    /** Forced visual state. Auto-detected from value if "default". */
    state?: ComposerState;
    /** Active assistant chip label. */
    assistantLabel?: string;
    /** Show keyboard hint (Shift+Enter). */
    hint?: boolean;
    onSubmit?: (value: string) => void;
    onAttach?: () => void;
    onSlash?: () => void;
    onMention?: () => void;
    onMic?: () => void;
    onRemoveAttachment?: (index: number) => void;
  }

  let {
    value = $bindable(""),
    placeholder = "Sur quoi concentrer nos efforts ensuite ?",
    attachments = [],
    slashCommands = [],
    mentions = [],
    state = "default",
    assistantLabel,
    hint = true,
    onSubmit,
    onAttach,
    onSlash,
    onMention,
    onMic,
    onRemoveAttachment,
  }: Props = $props();

  const isSlash = $derived(state === "slash");
  const hasValue = $derived(value.trim().length > 0);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (hasValue && onSubmit) onSubmit(value);
    }
  }
</script>

<div class="relative">
  {#if isSlash && slashCommands.length > 0}
    <div
      class="absolute bottom-full left-0 right-0 mb-1.5 bg-card border border-border rounded-[10px] p-1.5 shadow-elev-3 z-10"
    >
      <div
        class="text-[10px] tracking-[1.2px] text-muted-foreground font-semibold px-2 py-1.5 font-mono"
      >
        COMMANDES
      </div>
      {#each slashCommands as s}
        <div
          class="flex items-center gap-2 px-2 py-1.5 rounded-md {s.active
            ? 'bg-primary/10'
            : 'bg-transparent'}"
        >
          <code
            class="font-mono text-[11px] text-primary font-semibold w-[88px]"
          >
            {s.cmd}
          </code>
          <span class="text-[11.5px] text-muted-foreground">{s.desc}</span>
        </div>
      {/each}
    </div>
  {/if}

  {#if state === "mentions" && mentions.length > 0}
    <div
      class="absolute bottom-full left-0 right-0 mb-1.5 bg-card border border-border rounded-[10px] p-1.5 shadow-elev-3 z-10"
    >
      <div
        class="text-[10px] tracking-[1.2px] text-muted-foreground font-semibold px-2 py-1.5 font-mono"
      >
        ASSISTANTS
      </div>
      {#each mentions as m}
        <div
          class="flex items-center gap-2 px-2 py-1.5 rounded-md {m.active
            ? 'bg-secondary/10'
            : 'bg-transparent'}"
        >
          <Sparkles size={10} class="text-secondary-foreground" />
          <span class="text-[11.5px] font-medium text-foreground">@{m.name}</span>
          {#if m.description}
            <span class="text-[11px] text-muted-foreground">— {m.description}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div
    class="rounded-xl bg-card border transition-all {isSlash
      ? 'border-primary shadow-[0_0_0_3px_hsl(var(--primary)/0.1)]'
      : 'border-border shadow-elev-1'}"
  >
    {#if attachments.length > 0}
      <div class="px-3.5 pt-2.5 pb-1.5 flex gap-1.5 flex-wrap">
        {#each attachments as f, i}
          <div
            class="inline-flex items-center gap-1.5 pl-1.5 pr-2 py-1 rounded-md bg-primary/5 border border-primary/10 text-[11px]"
          >
            <div
              class="w-4 h-4 rounded-[3px] bg-primary/10 text-primary inline-flex items-center justify-center"
            >
              <File size={9} />
            </div>
            <span class="font-medium text-foreground">{f.name}</span>
            <span class="text-muted-foreground/70">{f.size}</span>
            <button
              type="button"
              onclick={() => onRemoveAttachment?.(i)}
              class="text-muted-foreground/70 hover:text-foreground bg-transparent border-0 p-0 cursor-pointer ml-0.5"
              aria-label="Retirer la pièce jointe"
            >
              <X size={10} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <div class="px-3.5 py-3 min-h-[42px]">
      <textarea
        bind:value
        onkeydown={handleKeydown}
        rows="1"
        {placeholder}
        class="w-full border-0 outline-none bg-transparent text-[13.5px] text-foreground font-[inherit] resize-none placeholder:text-muted-foreground/70"
      ></textarea>
    </div>

    <div
      class="px-2 pt-1.5 pb-2 flex items-center gap-0.5 border-t border-border/60"
    >
      <button
        type="button"
        onclick={onAttach}
        class="w-7 h-7 rounded-md border-0 bg-transparent text-muted-foreground hover:bg-muted hover:text-foreground inline-flex items-center justify-center cursor-pointer transition-colors"
        title="Joindre"
      >
        <Paperclip size={13} />
      </button>
      <button
        type="button"
        onclick={onSlash}
        class="w-7 h-7 rounded-md border-0 inline-flex items-center justify-center cursor-pointer transition-colors {isSlash
          ? 'bg-primary/10 text-primary'
          : 'bg-transparent text-muted-foreground hover:bg-muted hover:text-foreground'}"
        title="Commandes"
      >
        <Slash size={13} />
      </button>
      <button
        type="button"
        onclick={onMention}
        class="w-7 h-7 rounded-md border-0 inline-flex items-center justify-center cursor-pointer transition-colors {state ===
        'mentions'
          ? 'bg-primary/10 text-primary'
          : 'bg-transparent text-muted-foreground hover:bg-muted hover:text-foreground'}"
        title="Mentionner un agent"
      >
        <AtSign size={13} />
      </button>
      <button
        type="button"
        onclick={onMic}
        class="w-7 h-7 rounded-md border-0 bg-transparent text-muted-foreground hover:bg-muted hover:text-foreground inline-flex items-center justify-center cursor-pointer transition-colors"
        title="Dicter"
      >
        <Mic size={13} />
      </button>
      <div class="flex-1"></div>
      {#if hint}
        <span
          class="text-[10.5px] text-muted-foreground/70 mr-1.5 inline-flex items-center gap-1"
        >
          <Kbd>Shift</Kbd><Kbd>↵</Kbd> nouvelle ligne
        </span>
      {/if}
      {#if assistantLabel}
        <Chip size="sm" tone="neutral">{assistantLabel}</Chip>
      {/if}
      <button
        type="button"
        disabled={!hasValue}
        onclick={() => hasValue && onSubmit?.(value)}
        class="ml-1.5 w-[30px] h-[30px] rounded-lg border-0 inline-flex items-center justify-center transition-colors {hasValue
          ? 'bg-primary text-white cursor-pointer hover:bg-primary/90'
          : 'bg-muted text-muted-foreground/70 cursor-not-allowed'}"
        aria-label="Envoyer"
      >
        <ArrowUp size={14} />
      </button>
    </div>
  </div>
</div>
