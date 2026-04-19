<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { FileStack } from "lucide-svelte";
  import { currentSession } from "$lib/stores/chat";
  import {
    artifacts,
    clearArtifacts,
    loadArtifacts,
    requestChatInputAppend,
    selectedArtifact,
    selectedArtifactId,
    type Artifact,
  } from "$lib/stores/artifacts";
  import { detectAndPersist } from "$lib/chat/artifactDetect";
  import ArtifactListItem from "./ArtifactListItem.svelte";
  import ArtifactViewer from "./ArtifactViewer.svelte";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  let listEl = $state<HTMLDivElement | undefined>(undefined);

  // Reload artifacts when the session changes.
  $effect(() => {
    const sid = sessionId;
    selectedArtifactId.set(null);
    clearArtifacts();
    if (sid) void loadArtifacts(sid);
  });

  // Detection pass — runs whenever the message list of the current session
  // grows, so newly-produced tool outputs become artifacts automatically.
  $effect(() => {
    const session = $currentSession;
    if (!session || session.id !== sessionId) return;
    const messages = session.messages ?? [];
    const existing = get(artifacts);
    void detectAndPersist(sessionId, messages, existing);
  });

  function reinject(a: Artifact): void {
    requestChatInputAppend(`@artifact:${a.id}`);
  }

  // j/k navigation between artifacts in the list.
  function handleKey(e: KeyboardEvent): void {
    const list = get(artifacts);
    if (list.length === 0) return;
    const currentId = get(selectedArtifactId);
    if (e.key === "j" || e.key === "ArrowDown") {
      e.preventDefault();
      if (!currentId) {
        selectedArtifactId.set(list[0].id);
        return;
      }
      const idx = list.findIndex((a) => a.id === currentId);
      const next = list[Math.min(idx + 1, list.length - 1)];
      selectedArtifactId.set(next.id);
    } else if (e.key === "k" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!currentId) {
        selectedArtifactId.set(list[0].id);
        return;
      }
      const idx = list.findIndex((a) => a.id === currentId);
      const prev = list[Math.max(idx - 1, 0)];
      selectedArtifactId.set(prev.id);
    } else if (e.key === "Escape" && currentId) {
      e.preventDefault();
      selectedArtifactId.set(null);
    }
  }

  onMount(() => {
    const target = listEl;
    target?.focus();
  });
</script>

<div
  class="flex h-full min-h-0 flex-col"
  data-testid="artifacts-panel"
  role="region"
  aria-label="Artifacts"
>
  {#if $selectedArtifact}
    <ArtifactViewer
      artifact={$selectedArtifact}
      onback={() => selectedArtifactId.set(null)}
      onreinject={reinject}
    />
  {:else}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      bind:this={listEl}
      tabindex="0"
      role="listbox"
      aria-label="Artifacts list"
      onkeydown={handleKey}
      class="flex h-full min-h-0 flex-col gap-1 overflow-y-auto px-3 py-3 focus:outline-none"
      data-testid="artifacts-panel-list"
    >
      {#if $artifacts.length === 0}
        <div
          class="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center"
        >
          <FileStack class="h-5 w-5 text-muted-foreground/50" />
          <p class="text-[11px] text-muted-foreground/70">
            Aucun artefact pour cette session.
          </p>
          <p class="text-[10px] text-muted-foreground/50">
            Les sorties d'outils volumineuses (fichiers, blocs de code, logs
            bash) seront capturées ici automatiquement.
          </p>
        </div>
      {:else}
        {#each $artifacts as artifact (artifact.id)}
          <ArtifactListItem
            {artifact}
            active={$selectedArtifactId === artifact.id}
            onselect={() => selectedArtifactId.set(artifact.id)}
          />
        {/each}
      {/if}
    </div>
  {/if}
</div>
