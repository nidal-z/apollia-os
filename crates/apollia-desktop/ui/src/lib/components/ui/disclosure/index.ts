/**
 * Disclosure - controlled show/hide primitive.
 *
 * A `DisclosureRow` is just a `Disclosure` whose `summary` is a single row of
 * label + count, mirroring the chat ActivityStrip. Compose it inline:
 *
 *   <script lang="ts">
 *     import { Disclosure } from "$lib/components/ui/disclosure";
 *   </script>
 *
 *   <Disclosure open={false} testid="details">
 *     {#snippet summary()}
 *       <span class="font-semibold text-foreground">Details</span>
 *       <span class="text-muted-foreground/70">· 4 items</span>
 *     {/snippet}
 *     {#snippet children()}
 *       <p class="text-body-sm text-muted-foreground">Disclosed content.</p>
 *     {/snippet}
 *   </Disclosure>
 */
export { default as Disclosure } from "./Disclosure.svelte";
