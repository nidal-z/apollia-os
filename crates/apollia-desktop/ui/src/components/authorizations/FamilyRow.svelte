<script lang="ts">
  /**
   * FamilyRow - one action family an agent holds accesses in: what it covers
   * in plain words and what it asks. A family the organisation policy forbids
   * carries the mention and no action.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { FAMILY_ASK_LABEL, FAMILY_LABEL, type FamilyLine } from "./authorizations";

  interface Props {
    line: FamilyLine;
  }

  let { line }: Props = $props();

  const imposed = $derived(line.ask === "imposed");
</script>

<li
  class="flex items-start gap-3 border-b border-border/40 py-2.5 last:border-b-0"
  data-testid="authorizations-family"
  data-family={line.family}
  data-imposed={imposed}
>
  <div class="min-w-0 flex-1">
    <div class="text-body-sm {imposed ? 'text-muted-foreground' : 'text-foreground'}">
      {$t(FAMILY_LABEL[line.family])}
    </div>
    <div class="text-caption text-muted-foreground [overflow-wrap:anywhere]">
      {line.covers.join(", ")}
    </div>
  </div>
  <Badge
    size="sm"
    variant={imposed ? "warning" : "neutral"}
    outline
    data-testid="authorizations-family-ask"
  >
    {$t(FAMILY_ASK_LABEL[line.ask])}
  </Badge>
</li>
