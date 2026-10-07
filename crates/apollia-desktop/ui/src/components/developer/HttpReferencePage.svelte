<script lang="ts">
  /**
   * A page of the HTTP reference: the overview with its groups, or a group
   * with each of its operations (method, path, parameters, body, answers).
   */
  import { t } from "svelte-i18n";
  import { developerMode } from "$lib/stores/developerMode";
  import ExplainedBadge from "../ai/ExplainedBadge.svelte";
  import {
    METHOD_EXPLANATIONS,
    PLACE_LABELS,
    groupSlug,
    operationCount,
    type ReferencePage,
  } from "./httpReference";

  interface Props {
    /** The page shown. */
    page: ReferencePage;
    /** Called with the route of the group the user chose on the overview. */
    onopen: (route: string) => void;
  }

  let { page, onopen }: Props = $props();

  const CELL = "px-3 py-2 text-left align-top";
</script>

{#if $developerMode}
  <div class="flex flex-col gap-4" data-testid="developer-reference-page" data-reference={page.kind}>
    {#if page.kind === "overview"}
      {@const reference = page.reference}
      <div class="flex flex-col gap-1">
        <p class="text-body-md text-foreground">{reference.title}</p>
        {#if reference.description !== ""}
          <p class="text-body-sm text-muted-foreground">{reference.description}</p>
        {/if}
        <p class="text-body-sm text-muted-foreground">
          {$t("dev.developer.docs.reference.generated")}
          {$t("dev.developer.docs.reference.totals", {
            values: {
              operations: operationCount(reference),
              groups: reference.groups.length,
              version: reference.version,
            },
          })}
        </p>
      </div>
      <ul class="flex flex-col gap-2">
        {#each reference.groups as group (group.route)}
          <li>
            <button
              type="button"
              class="flex w-full flex-wrap items-baseline gap-x-3 gap-y-1 rounded-md border border-border px-4 py-3 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              onclick={() => onopen(group.route)}
              data-testid={`developer-reference-group-${groupSlug(group)}`}
            >
              <span class="text-body-sm font-medium text-foreground">{group.title}</span>
              <span class="min-w-0 flex-1 text-caption text-muted-foreground">{group.description}</span>
              <span class="shrink-0 text-caption text-muted-foreground">
                {$t("dev.developer.docs.reference.operations", {
                  values: { count: group.operations.length },
                })}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      {@const group = page.group}
      <div class="flex flex-col gap-1">
        {#if group.description !== ""}
          <p class="text-body-sm text-muted-foreground">{group.description}</p>
        {/if}
        <p class="text-caption text-muted-foreground">
          {$t("dev.developer.docs.reference.generated")}
        </p>
      </div>
      {#each group.operations as operation (`${operation.method} ${operation.path}`)}
        <article
          class="flex flex-col gap-2 rounded-md border border-border p-4"
          data-testid="developer-reference-operation"
          data-operation={operation.id}
        >
          <header class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <ExplainedBadge
              label={operation.method.toUpperCase()}
              explanation={$t(METHOD_EXPLANATIONS[operation.method])}
              testid="developer-reference-method"
            />
            <code class="min-w-0 break-all font-mono text-body-sm text-foreground" data-reference-path>
              {operation.path}
            </code>
            {#if operation.summary !== ""}
              <span class="text-body-sm text-muted-foreground">{operation.summary}</span>
            {/if}
          </header>
          {#if operation.description !== ""}
            <p class="whitespace-pre-line text-caption text-muted-foreground">{operation.description}</p>
          {/if}

          {#if operation.body}
            <p class="text-caption text-muted-foreground" data-reference-body>
              {$t(
                operation.body.required
                  ? "dev.developer.docs.reference.body_required"
                  : "dev.developer.docs.reference.body_optional",
              )}
              <code class="font-mono text-foreground">{operation.body.type}</code>
              ({operation.body.mediaType})
            </p>
          {/if}

          {#if operation.parameters.length > 0}
            <h3 class="text-caption text-muted-foreground">
              {$t("dev.developer.docs.reference.parameters")}
            </h3>
            <div class="overflow-x-auto rounded-md border border-border">
              <table class="w-full border-collapse text-caption" data-reference-parameters>
                <thead class="bg-muted text-muted-foreground">
                  <tr>
                    <th scope="col" class={CELL}>{$t("dev.developer.docs.reference.column.name")}</th>
                    <th scope="col" class={CELL}>{$t("dev.developer.docs.reference.column.where")}</th>
                    <th scope="col" class={CELL}>{$t("dev.developer.docs.reference.column.type")}</th>
                    <th scope="col" class={CELL}>
                      {$t("dev.developer.docs.reference.column.required")}
                    </th>
                    <th scope="col" class={CELL}>
                      {$t("dev.developer.docs.reference.column.description")}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {#each operation.parameters as parameter (`${parameter.where}:${parameter.name}`)}
                    <tr class="border-t border-border text-foreground">
                      <td class={`${CELL} font-mono`}>{parameter.name}</td>
                      <td class={CELL}>{$t(PLACE_LABELS[parameter.where])}</td>
                      <td class={`${CELL} font-mono`}>{parameter.type}</td>
                      <td class={CELL}>
                        {$t(
                          parameter.required
                            ? "dev.developer.docs.reference.yes"
                            : "dev.developer.docs.reference.no",
                        )}
                      </td>
                      <td class={CELL}>{parameter.description}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <p class="text-caption text-muted-foreground">
              {$t("dev.developer.docs.reference.no_parameters")}
            </p>
          {/if}

          {#if operation.responses.length > 0}
            <h3 class="text-caption text-muted-foreground">
              {$t("dev.developer.docs.reference.responses")}
            </h3>
            <ul class="flex flex-col gap-1 text-caption" data-reference-responses>
              {#each operation.responses as response (response.status)}
                <li class="flex flex-wrap items-baseline gap-x-2 text-foreground">
                  <span class="font-mono">{response.status}</span>
                  <span class="text-muted-foreground">{response.description}</span>
                  {#if response.type !== null}
                    <code class="font-mono text-muted-foreground">{response.type}</code>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </article>
      {/each}
    {/if}
  </div>
{/if}
