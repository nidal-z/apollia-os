<script lang="ts" module>
  /** A line of a content group. */
  export interface ContentLine {
    id: string;
    name: string;
    description: string;
    /** A short tag at the end of the line: an action family, "stays". */
    tag?: string;
  }
</script>

<script lang="ts">
  /**
   * One kind of content of an open package: a title and its lines. A line
   * carries a name, what it is for, an optional tag, and, for an agent or a
   * workflow, the gesture that opens its sheet in the Agents screen.
   */
  import { t } from "svelte-i18n";
  import type { ComponentType } from "svelte"; // NOSONAR typescript:S1874
  import { Button } from "$lib/components/ui/button";

  interface Props {
    kind: "agent" | "workflow" | "tool" | "connection";
    title: string;
    icon: ComponentType; // NOSONAR typescript:S1874
    lines: ContentLine[];
    /** Opens the sheet of a line; the gesture is offered only when given. */
    onopen?: (id: string) => void;
  }

  let { kind, title, icon: Icon, lines, onopen }: Props = $props();
</script>

{#if lines.length > 0}
  <section class="flex flex-col gap-2" data-testid="library-content-{kind}">
    <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
      {title}
    </h3>
    <ul class="flex flex-col gap-1.5">
      {#each lines as line (line.id)}
        <li
          class="flex items-center gap-2.5 rounded-md border border-border bg-card px-3 py-2"
          data-testid="library-content-item"
          data-item={line.id}
        >
          <Icon size={15} class="shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="truncate text-body-sm font-medium text-foreground">{line.name}</p>
            {#if line.description}
              <p class="line-clamp-2 text-caption text-muted-foreground">{line.description}</p>
            {/if}
          </div>
          {#if line.tag}
            <span class="shrink-0 text-caption text-muted-foreground" data-testid="library-content-tag">
              {line.tag}
            </span>
          {/if}
          {#if onopen}
            <Button
              variant="ghost"
              size="sm"
              onclick={() => onopen(line.id)}
              data-testid="library-open-sheet"
              data-item={line.id}
            >
              {$t("library.detail.open_sheet")}
            </Button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}
