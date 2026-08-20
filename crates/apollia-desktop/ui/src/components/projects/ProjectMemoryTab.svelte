<script lang="ts">
  /**
   * ProjectMemoryTab - attached documents plus project-scoped memory namespaces.
   *
   * Two stacked sections: the documents joined to the project, which this tab
   * also owns the mutations for (attach through the native file picker, detach
   * per row), and the aggregated `{project_id}:*` memory namespaces with their
   * entry counts. Failures surface inline through the page `ErrorBanner`, the
   * parent reloads the project detail via `onDocumentsChanged`.
   *
   * The detach uses the two-step inline confirm of `AgentDetailHeader`: the
   * first click arms the row, the second one commits. It only drops the
   * project's reference to the file, which stays where it is on disk.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Folder, FolderOpen, Paperclip, X } from "lucide-svelte";
  import {
    SectionTitle,
    EmptyState,
    ErrorBanner,
    SkeletonList,
  } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import {
    uploadProjectDocument,
    deleteProjectDocument,
  } from "$lib/ipc/projects";
  import type { ProjectDocument } from "$lib/types";

  interface MemoryNamespace {
    namespace: string;
    subname: string;
    count: number;
  }

  interface Props {
    projectId: string;
    documents: ProjectDocument[];
    namespaces: MemoryNamespace[];
    loading: boolean;
    /** Called after an attach or a detach so the parent can reload the detail. */
    onDocumentsChanged: () => void | Promise<void>;
  }

  let { projectId, documents, namespaces, loading, onDocumentsChanged }: Props =
    $props();

  let documentError = $state<HumanizedError | null>(null);
  let attaching = $state(false);
  let removingDocId = $state<string | null>(null);
  // Id of the row whose detach confirm is armed, null when none is.
  let confirmingDocId = $state<string | null>(null);

  // An armed row that leaves the list (removed elsewhere, reload) must not stay
  // armed for whatever document lands at that position next.
  $effect(() => {
    if (confirmingDocId && !documents.some((d) => d.id === confirmingDocId)) {
      confirmingDocId = null;
    }
  });

  async function attachDocument(): Promise<void> {
    documentError = null;
    // Armed before the native picker opens: the button drives the whole
    // gesture, so it stays disabled while the dialog is up.
    attaching = true;
    try {
      // Filtered, because the runtime reads an attached document with
      // read_to_string and skips anything that is not valid UTF-8 text. An
      // unfiltered picker let a PDF through, answered with a green toast, and
      // the file never reached a single conversation.
      const picked = await openDialog({
        multiple: false,
        filters: [
          {
            name: $t("projects.document_filter_text"),
            extensions: [
              "md", "markdown", "txt", "rst", "adoc",
              "json", "yaml", "yml", "toml", "csv", "tsv",
              "html", "xml", "log",
            ],
          },
        ],
      });
      if (typeof picked !== "string") return;
      const doc = await uploadProjectDocument(projectId, picked);
      addToast(
        $t("projects.document_added", { values: { name: doc.name } }),
        "success",
      );
      await onDocumentsChanged();
    } catch (err: unknown) {
      documentError = reportError(err, { surface: "inline" });
    } finally {
      attaching = false;
    }
  }

  async function detachDocument(doc: ProjectDocument): Promise<void> {
    documentError = null;
    confirmingDocId = null;
    removingDocId = doc.id;
    try {
      await deleteProjectDocument(doc.id);
      addToast($t("projects.document_removed"), "success");
      await onDocumentsChanged();
    } catch (err: unknown) {
      documentError = reportError(err, { surface: "inline" });
    } finally {
      removingDocId = null;
    }
  }
</script>

<SectionTitle count={documents.length}>
  {$t("projects.memory_documents")}
  {#snippet action()}
    <Button
      variant="outline"
      size="sm"
      type="button"
      onclick={attachDocument}
      disabled={attaching}
      data-testid="project-doc-attach-btn"
    >
      {#snippet icon()}<Paperclip size={12} />{/snippet}
      {attaching
        ? $t("projects.attaching_document")
        : $t("projects.attach_document")}
    </Button>
  {/snippet}
</SectionTitle>
<div class="px-8 pb-4 space-y-3">
  {#if documentError}
    <ErrorBanner data-testid="project-doc-error">
      <p class="font-medium">{documentError.friendly_message}</p>
      <p class="text-caption opacity-90">{documentError.suggested_action}</p>
      {#if documentError.detail}
        <details class="mt-2">
          <summary
            class="cursor-pointer text-caption opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            data-testid="project-doc-error-details"
          >
            {$t("errors.show_details")}
          </summary>
          <p class="mt-1.5 break-all font-mono text-code-sm opacity-80">
            {documentError.detail}
          </p>
        </details>
      {/if}
    </ErrorBanner>
  {/if}

  {#if documents.length === 0}
    <EmptyState
      title={$t("projects.empty_memory_title")}
      desc={$t("projects.empty_documents_desc")}
    >
      {#snippet icon()}<Folder size={20} />{/snippet}
      {#snippet action()}
        <Button
          variant="primary-solid"
          size="sm"
          type="button"
          onclick={attachDocument}
          disabled={attaching}
          data-testid="project-doc-attach-empty-btn"
        >
          {#snippet icon()}<Paperclip size={12} />{/snippet}
          {$t("projects.attach_document")}
        </Button>
      {/snippet}
    </EmptyState>
  {:else}
    <ul class="divide-y divide-border border border-border rounded-xl bg-card overflow-hidden">
      {#each documents as doc (doc.id)}
        <li class="px-4 py-2.5 flex items-center gap-2.5">
          <Folder size={13} class="text-muted-foreground" />
          {#if confirmingDocId === doc.id}
            <span class="text-caption font-medium text-destructive truncate flex-1">
              {$t("projects.detach_document_confirm", { values: { name: doc.name } })}
            </span>
            <Button
              variant="outline"
              size="sm"
              type="button"
              onclick={() => (confirmingDocId = null)}
              data-testid="project-doc-remove-cancel-{doc.id}"
            >
              {$t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              type="button"
              onclick={() => detachDocument(doc)}
              disabled={removingDocId === doc.id}
              data-testid="project-doc-remove-confirm-{doc.id}"
            >
              {#snippet icon()}<X size={12} />{/snippet}
              {$t("projects.detach_document_confirm_action")}
            </Button>
          {:else}
            <span class="text-body-xs text-foreground truncate flex-1">{doc.name}</span>
            <span class="text-caption text-muted-foreground font-mono tabular-nums">
              {(doc.size_bytes / 1024).toFixed(1)} {$t("tools.body.unit_kb")}
            </span>
            <Button
              variant="ghost"
              size="icon-sm"
              type="button"
              onclick={() => (confirmingDocId = doc.id)}
              disabled={removingDocId === doc.id}
              aria-label={$t("projects.remove_document")}
              title={$t("projects.remove_document")}
              data-testid="project-doc-remove-{doc.id}"
            >
              <X size={12} />
            </Button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<SectionTitle count={namespaces.length}>
  {$t("projects.memory_namespaces")}
</SectionTitle>
<div class="px-8 pb-6">
  {#if loading}
    <SkeletonList count={2} avatar={false} rowClass="px-4 py-2" />
  {:else if namespaces.length === 0}
    <EmptyState
      title={$t("projects.no_project_memory")}
      desc={$t("projects.no_project_memory_desc")}
    >
      {#snippet icon()}<FolderOpen size={20} />{/snippet}
    </EmptyState>
  {:else}
    <ul class="divide-y divide-border border border-border rounded-xl bg-card overflow-hidden">
      {#each namespaces as ns (ns.namespace)}
        <li class="px-4 py-2.5 flex items-center gap-2.5">
          <FolderOpen size={13} class="text-muted-foreground" />
          <span class="text-body-xs text-foreground truncate flex-1">{ns.subname}</span>
          <span class="text-caption text-muted-foreground font-mono truncate">{ns.namespace}</span>
          <span class="text-caption text-muted-foreground font-medium tabular-nums">{ns.count}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>
