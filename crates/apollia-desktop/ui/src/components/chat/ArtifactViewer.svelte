<script lang="ts">
  import {
    ArrowLeft,
    Copy,
    Download,
    MessageSquarePlus,
    Pencil,
    Save,
    Trash2,
    X,
  } from "lucide-svelte";
  import {
    deleteArtifact,
    updateArtifact,
    type Artifact,
  } from "$lib/stores/artifacts";

  interface Props {
    artifact: Artifact;
    onback: () => void;
    /** Inject an `@artifact:<id>` reference into the chat input. */
    onreinject: (artifact: Artifact) => void;
  }

  let { artifact, onback, onreinject }: Props = $props();

  let editing = $state(false);
  let draftTitle = $state(artifact.title);
  let draftContent = $state(artifact.content);
  let saving = $state(false);
  let copyState = $state<"idle" | "done">("idle");

  $effect(() => {
    // Reset drafts when switching artifact.
    draftTitle = artifact.title;
    draftContent = artifact.content;
    editing = false;
  });

  async function copy(): Promise<void> {
    await navigator.clipboard.writeText(artifact.content);
    copyState = "done";
    setTimeout(() => (copyState = "idle"), 1200);
  }

  function download(): void {
    const blob = new Blob([artifact.content], {
      type: "text/plain;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = safeFilename(artifact);
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  async function save(): Promise<void> {
    saving = true;
    try {
      await updateArtifact(artifact.id, {
        title: draftTitle,
        content: draftContent,
      });
      editing = false;
    } finally {
      saving = false;
    }
  }

  async function remove(): Promise<void> {
    await deleteArtifact(artifact.id);
    onback();
  }

  function safeFilename(a: Artifact): string {
    const base =
      a.title.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") ||
      "artifact";
    if (/\.[a-z0-9]+$/i.test(base)) return base;
    const ext =
      a.language === "rust"
        ? ".rs"
        : a.language === "typescript"
          ? ".ts"
          : a.language === "javascript"
            ? ".js"
            : a.language === "python"
              ? ".py"
              : a.language === "markdown"
                ? ".md"
                : a.language === "json"
                  ? ".json"
                  : a.language === "toml"
                    ? ".toml"
                    : a.language === "yaml"
                      ? ".yml"
                      : a.language === "bash"
                        ? ".sh"
                        : ".txt";
    return `${base}${ext}`;
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="artifact-viewer">
  <div class="flex items-center gap-1 border-b border-border/30 px-2 py-1.5">
    <button
      type="button"
      onclick={onback}
      class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
      aria-label="Retour à la liste"
      data-testid="artifact-viewer-back"
    >
      <ArrowLeft size={12} />
      Retour
    </button>
    <div class="flex-1"></div>
    {#if !editing}
      <button
        type="button"
        onclick={copy}
        class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
        aria-label="Copier"
        data-testid="artifact-viewer-copy"
      >
        <Copy size={12} />
        {copyState === "done" ? "Copié" : "Copier"}
      </button>
      <button
        type="button"
        onclick={download}
        class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
        aria-label="Télécharger"
        data-testid="artifact-viewer-download"
      >
        <Download size={12} />
        Télécharger
      </button>
      <button
        type="button"
        onclick={() => onreinject(artifact)}
        class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
        aria-label="Réinjecter dans le chat"
        data-testid="artifact-viewer-reinject"
      >
        <MessageSquarePlus size={12} />
        Réinjecter
      </button>
      <button
        type="button"
        onclick={() => (editing = true)}
        class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
        aria-label="Éditer"
        data-testid="artifact-viewer-edit"
      >
        <Pencil size={12} />
        Éditer
      </button>
      <button
        type="button"
        onclick={remove}
        class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
        aria-label="Supprimer"
        data-testid="artifact-viewer-delete"
      >
        <Trash2 size={12} />
      </button>
    {:else}
      <button
        type="button"
        onclick={save}
        disabled={saving}
        class="inline-flex h-7 items-center gap-1 rounded-md bg-primary/10 px-2 text-[11px] text-primary hover:bg-primary/20 disabled:opacity-50"
        data-testid="artifact-viewer-save"
      >
        <Save size={12} />
        {saving ? "Enregistrement…" : "Enregistrer"}
      </button>
      <button
        type="button"
        onclick={() => {
          editing = false;
          draftTitle = artifact.title;
          draftContent = artifact.content;
        }}
        class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-muted/40 hover:text-foreground"
        aria-label="Annuler"
        data-testid="artifact-viewer-cancel"
      >
        <X size={12} />
      </button>
    {/if}
  </div>

  <div class="border-b border-border/30 px-3 py-2">
    {#if editing}
      <input
        type="text"
        bind:value={draftTitle}
        class="w-full rounded-md border border-border/40 bg-background px-2 py-1 text-[12px] font-medium focus:border-primary focus:outline-none"
        data-testid="artifact-viewer-title-input"
      />
    {:else}
      <p
        class="truncate text-[12px] font-medium text-foreground"
        title={artifact.title}
      >
        {artifact.title}
      </p>
    {/if}
    <div
      class="mt-1 flex flex-wrap items-center gap-x-2 text-[10px] text-muted-foreground/70"
    >
      <span>{artifact.kind}</span>
      {#if artifact.language}<span>· {artifact.language}</span>{/if}
      {#if artifact.source_tool}<span>· {artifact.source_tool}</span>{/if}
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-hidden">
    {#if editing}
      <textarea
        bind:value={draftContent}
        class="h-full w-full resize-none bg-background px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground focus:outline-none"
        spellcheck="false"
        data-testid="artifact-viewer-editor"
      ></textarea>
    {:else}
      <pre
        class="h-full overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground"
        data-testid="artifact-viewer-content">{artifact.content}</pre>
    {/if}
  </div>
</div>
