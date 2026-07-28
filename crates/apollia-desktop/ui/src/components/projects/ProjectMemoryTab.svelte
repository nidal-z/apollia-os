<script lang="ts">
  /**
   * ProjectMemoryTab - attached documents plus project-scoped memory namespaces.
   *
   * Two stacked sections: static documents joined to the project, and the
   * aggregated `{project_id}:*` memory namespaces with their entry counts.
   */
  import { t } from "svelte-i18n";
  import { Folder, FolderOpen } from "lucide-svelte";
  import { SectionTitle, EmptyState, SkeletonList } from "$lib/components/operator";
  import type { ProjectDocument } from "$lib/types";

  interface MemoryNamespace {
    namespace: string;
    subname: string;
    count: number;
  }

  interface Props {
    documents: ProjectDocument[];
    namespaces: MemoryNamespace[];
    loading: boolean;
  }

  let { documents, namespaces, loading }: Props = $props();
</script>

<SectionTitle count={documents.length}>
  {$t("projects.memory_documents")}
</SectionTitle>
<div class="px-8 pb-4">
  {#if documents.length === 0}
    <EmptyState
      title={$t("projects.empty_memory_title")}
      desc={$t("projects.empty_memory_desc")}
    >
      {#snippet icon()}<Folder size={20} />{/snippet}
    </EmptyState>
  {:else}
    <ul class="divide-y divide-border border border-border rounded-xl bg-card overflow-hidden">
      {#each documents as doc (doc.id)}
        <li class="px-4 py-2.5 flex items-center gap-2.5">
          <Folder size={13} class="text-muted-foreground" />
          <span class="text-body-xs text-foreground truncate flex-1">{doc.name}</span>
          <span class="text-caption text-muted-foreground font-mono tabular-nums">
            {(doc.size_bytes / 1024).toFixed(1)} KB
          </span>
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
