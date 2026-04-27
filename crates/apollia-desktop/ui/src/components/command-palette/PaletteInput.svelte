<script lang="ts">
  /**
   * App-level palette input. The existing `<Command>`
   * primitive owns its own input for accessibility/focus-trap reasons,
   * so this wrapper exists as a stable surface for future decoupling
   * (e.g. if the palette moves to a custom renderer that does not embed
   * its own input).
   */
  import { Search as SearchIcon } from "lucide-svelte";

  interface Props {
    placeholder: string;
    value: string;
  }

  let { placeholder, value = $bindable("") }: Props = $props();
</script>

<label
  class="flex items-center gap-2 px-3.5 py-2.5 text-sm text-card-foreground"
>
  <span class="text-muted-foreground" aria-hidden="true">
    <SearchIcon size={14} strokeWidth={1.75} />
  </span>
  <input
    type="text"
    role="combobox"
    aria-autocomplete="list"
    aria-expanded="true"
    {placeholder}
    bind:value
    class="flex-1 bg-transparent text-sm text-card-foreground placeholder:text-muted-foreground focus:outline-none"
  />
</label>
