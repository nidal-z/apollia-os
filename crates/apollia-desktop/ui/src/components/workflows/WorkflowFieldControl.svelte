<script lang="ts">
  /**
   * WorkflowFieldControl - one input of a workflow in its launch form: the
   * label, the control its type calls for, its help, and why its value is
   * refused when it is.
   *
   * A line of text, a text over several lines, a file chosen on this
   * machine, a day, a number, a yes or no, one choice among several, several
   * choices, a list of values added one by one, and a value given as JSON for
   * a type no control carries.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { X } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { DatePicker } from "$lib/components/ui/date-picker";
  import { FormField } from "$lib/components/ui/form-field";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Toggle } from "$lib/components/ui/toggle";
  import { reportError } from "$lib/errors/reportError";
  import type { WorkflowField } from "$lib/ipc/workflows";
  import {
    addItem,
    controlOf,
    FIELD_ERROR_LABEL,
    toggleItem,
    type FieldErrorCode,
    type FieldValue,
  } from "./workflowForm";

  interface Props {
    field: WorkflowField;
    value: FieldValue;
    /** Why the value is refused, when it is. */
    error: FieldErrorCode | null;
    onchange: (value: FieldValue) => void;
  }

  let { field, value, error, onchange }: Props = $props();

  const control = $derived(controlOf(field));
  const id = $derived(`workflow-input-${field.name}`);
  const text = $derived(typeof value === "string" ? value : "");
  const items = $derived(Array.isArray(value) ? value : []);
  const errorText = $derived(error ? $t(FIELD_ERROR_LABEL[error]) : undefined);

  /** The value typed for the next item of a list. */
  let draft = $state("");

  async function chooseFile(): Promise<void> {
    try {
      const path = await openDialog({ multiple: false });
      if (typeof path === "string") onchange(path);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  function addDraft(): void {
    onchange(addItem(items, draft));
    draft = "";
  }
</script>

<FormField
  {id}
  label={field.name}
  hint={field.description}
  error={errorText}
  required={field.required}
  optional={!field.required}
  class={control === "textarea" || control === "json" || control === "items" ? "sm:col-span-2" : ""}
  data-testid="workflow-field"
>
  <div data-field={field.name} data-control={control} data-invalid={error !== null}>
    {#if control === "textarea"}
      <Textarea
        {id}
        rows={4}
        value={text}
        oninput={(e) => onchange(e.currentTarget.value)}
        data-testid="workflow-input-{field.name}"
      />
    {:else if control === "json"}
      <Textarea
        {id}
        rows={3}
        class="font-mono"
        value={text}
        oninput={(e) => onchange(e.currentTarget.value)}
        data-testid="workflow-input-{field.name}"
      />
    {:else if control === "file"}
      <div class="flex items-center gap-2">
        <Input
          {id}
          value={text}
          readonly
          placeholder={$t("workflows.file_none")}
          class="min-w-0 flex-1"
          data-testid="workflow-input-{field.name}"
        />
        <Button type="button" variant="outline" size="sm" onclick={chooseFile} data-testid="workflow-input-{field.name}-choose">
          {$t("workflows.file_choose")}
        </Button>
      </div>
    {:else if control === "date"}
      <DatePicker
        {id}
        value={text}
        oninput={(e) => onchange(e.currentTarget.value)}
        data-testid="workflow-input-{field.name}"
      />
    {:else if control === "number"}
      <Input
        {id}
        inputmode={field.kind === "integer" ? "numeric" : "decimal"}
        value={text}
        oninput={(e) => onchange(e.currentTarget.value)}
        data-testid="workflow-input-{field.name}"
      />
    {:else if control === "toggle"}
      <Toggle
        {id}
        checked={value === true}
        onchange={(on) => onchange(on)}
        aria-label={field.name}
        data-testid="workflow-input-{field.name}"
      />
    {:else if control === "select"}
      <Select
        {id}
        value={text}
        onchange={(e) => onchange(e.currentTarget.value)}
        data-testid="workflow-input-{field.name}"
      >
        <option value="">{$t("workflows.choice_none")}</option>
        {#each field.choices ?? [] as choice (choice)}
          <option value={choice}>{choice}</option>
        {/each}
      </Select>
    {:else if control === "checks"}
      <ul class="m-0 list-none space-y-1 p-0" data-testid="workflow-input-{field.name}">
        {#each field.choices ?? [] as choice (choice)}
          <li class="flex items-center gap-2">
            <Checkbox
              id="{id}-{choice}"
              checked={items.includes(choice)}
              onchange={(on) => onchange(toggleItem(items, choice, on))}
              data-testid="workflow-input-{field.name}-check"
            />
            <label for="{id}-{choice}" class="text-body-xs text-foreground">{choice}</label>
          </li>
        {/each}
      </ul>
    {:else if control === "items"}
      <div class="space-y-2" data-testid="workflow-input-{field.name}">
        {#if items.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each items as item (item)}
              <Badge variant="neutral" size="sm" class="gap-1">
                {item}
                <Button
                  type="button"
                  variant="ghost"
                  size="auto"
                  onclick={() => onchange(toggleItem(items, item, false))}
                  aria-label={$t("workflows.item_remove", { values: { item } })}
                  data-testid="workflow-input-{field.name}-remove"
                >
                  <X size={11} />
                </Button>
              </Badge>
            {/each}
          </div>
        {/if}
        <div class="flex items-center gap-2">
          <Input
            {id}
            bind:value={draft}
            placeholder={$t("workflows.item_placeholder")}
            class="min-w-0 flex-1"
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addDraft();
              }
            }}
            data-testid="workflow-input-{field.name}-draft"
          />
          <Button type="button" variant="outline" size="sm" onclick={addDraft} data-testid="workflow-input-{field.name}-add">
            {$t("workflows.item_add")}
          </Button>
        </div>
      </div>
    {:else}
      <Input
        {id}
        value={text}
        oninput={(e) => onchange(e.currentTarget.value)}
        data-testid="workflow-input-{field.name}"
      />
    {/if}
  </div>
</FormField>
