<script lang="ts">
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";
  import { TimePicker } from "$lib/components/ui/date-picker";
  import { stripSecondsField } from "$lib/automations/humanize";
  import {
    DAYS_CRON,
    buildCronExpression,
    utcToLocal,
    type CronDraft,
    type CronPreset,
  } from "./cronExpression";

  interface Props {
    value: string;
    onchange: (expr: string) => void;
  }

  let { value, onchange }: Props = $props();

  type Preset = CronPreset;

  // cron day-of-week: 0=Sun,1=Mon,...,6=Sat - displayed as Mon-Sun
  const DAYS_LABEL = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

  let preset = $state<Preset>("custom");
  let dailyTime = $state("08:00");
  let weeklyTime = $state("08:00");
  let weeklyDays = $state([true, true, true, true, true, false, false]); // Mon-Fri
  let rawCron = $state("");
  let initialized = $state(false);

  function detectPreset(expr: string): Preset {
    if (!expr) return "custom";
    if (expr === "*/15 * * * *") return "15m";
    if (expr === "*/30 * * * *") return "30m";
    if (expr === "0 * * * *") return "hourly";
    if (/^\d+ \d+ \* \* \*$/.test(expr)) return "daily";
    if (/^\d+ \d+ \* \* [\d,]+$/.test(expr)) return "weekly";
    return "custom";
  }

  // The conversion between the local wall-clock time the pickers show and the
  // UTC the trigger engine evaluates lives in `cronExpression.ts`, which takes
  // the offset as an argument so it stays testable outside a browser.
  function tzOffsetMinutes(): number {
    // getTimezoneOffset returns minutes to add to local time to reach UTC.
    return new Date().getTimezoneOffset();
  }

  const pad = (n: number) => String(n).padStart(2, "0");

  function initFromValue(expr: string) {
    if (!expr) return;
    // The runtime persists scheduler presets in 6-field form (leading seconds
    // field); the builder reasons in the 5-field form it emits.
    const fiveField = stripSecondsField(expr);
    const p = detectPreset(fiveField);
    preset = p;
    rawCron = expr;
    if (p === "daily") {
      const [min, hour] = fiveField.split(" ").map(Number);
      const l = utcToLocal(hour, min, tzOffsetMinutes());
      dailyTime = `${pad(l.hh)}:${pad(l.mm)}`;
    } else if (p === "weekly") {
      const parts = fiveField.split(" ");
      const l = utcToLocal(Number(parts[1]), Number(parts[0]), tzOffsetMinutes());
      weeklyTime = `${pad(l.hh)}:${pad(l.mm)}`;
      const utcDays = parts[4].split(",").map(Number);
      const localDays = utcDays.map(d => ((d + l.dayDelta) % 7 + 7) % 7);
      weeklyDays = DAYS_CRON.map(d => localDays.includes(d));
    }
    initialized = true;
  }

  $effect(() => {
    if (!initialized) {
      initFromValue(value);
    }
  });

  function draft(p: Preset): CronDraft {
    return { preset: p, dailyTime, weeklyTime, weeklyDays: [...weeklyDays], rawCron };
  }

  /**
   * Emits the expression the current draft stands for, or an empty string when
   * it stands for none. An empty schedule is what both calling forms already
   * refuse, so a draft with no day ticked cannot reach the trigger repository.
   */
  function emit(p: Preset) {
    onchange(buildCronExpression(draft(p), tzOffsetMinutes()).expr);
  }

  /** Set while the weekly preset holds no day, so the row says why nothing is emitted. */
  const weeklyErrorKey = $derived(
    preset === "weekly" ? buildCronExpression(draft("weekly"), tzOffsetMinutes()).errorKey : null,
  );

  function selectPreset(p: Preset) {
    preset = p;
    emit(p);
  }

  function onDailyTimeChange(t: string) {
    dailyTime = t;
    if (preset === "daily") emit("daily");
  }

  function onWeeklyTimeChange(t: string) {
    weeklyTime = t;
    if (preset === "weekly") emit("weekly");
  }

  function toggleDay(i: number) {
    weeklyDays[i] = !weeklyDays[i];
    if (preset === "weekly") emit("weekly");
  }

  function onRawCronChange(val: string) {
    rawCron = val;
    if (preset === "custom") onchange(val);
  }

  const PRESETS: { key: Preset; labelKey: string }[] = [
    { key: "15m", labelKey: "triggers.cron_preset_every_15m" },
    { key: "30m", labelKey: "triggers.cron_preset_every_30m" },
    { key: "hourly", labelKey: "triggers.cron_preset_hourly" },
    { key: "daily", labelKey: "triggers.cron_preset_daily" },
    { key: "weekly", labelKey: "triggers.cron_preset_weekly" },
    { key: "custom", labelKey: "triggers.cron_preset_custom" },
  ];
</script>

<div class="space-y-3">
  <!-- Preset buttons -->
  <div class="flex flex-wrap gap-1.5">
    {#each PRESETS as p}
      <button
        type="button"
        class="rounded-full border px-3 py-1 text-xs font-medium transition-colors
          {preset === p.key
            ? 'border-primary bg-primary/10 text-primary'
            : 'border-border text-muted-foreground hover:border-border/60 hover:text-foreground'}"
        onclick={() => selectPreset(p.key)}
      >
        {$t(p.labelKey)}
      </button>
    {/each}
  </div>

  <!-- Daily: time picker -->
  {#if preset === "daily"}
    <div class="flex items-center gap-2">
      <span class="min-w-[2rem] text-xs text-muted-foreground">{$t("triggers.cron_at_time")}</span>
      <TimePicker
        value={dailyTime}
        onchange={(e) => onDailyTimeChange((e.target as HTMLInputElement).value)}
        class="w-32"
      />
    </div>
  {/if}

  <!-- Weekly: day chips + time picker -->
  {#if preset === "weekly"}
    <div class="space-y-2">
      <div class="flex items-center gap-1.5 flex-wrap">
        <span class="min-w-[2rem] text-xs text-muted-foreground">{$t("triggers.cron_on_days")}</span>
        {#each DAYS_LABEL as day, i}
          <button
            type="button"
            class="rounded-full border px-2.5 py-0.5 text-xs font-medium transition-colors
              {weeklyDays[i]
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-border text-muted-foreground hover:border-border/60'}"
            onclick={() => toggleDay(i)}
          >
            {day}
          </button>
        {/each}
      </div>
      {#if weeklyErrorKey}
        <p class="text-xs text-destructive" data-testid="cron-weekly-days-error">
          {$t(weeklyErrorKey)}
        </p>
      {/if}
      <div class="flex items-center gap-2">
        <span class="min-w-[2rem] text-xs text-muted-foreground">{$t("triggers.cron_at_time")}</span>
        <TimePicker
          value={weeklyTime}
          onchange={(e) => onWeeklyTimeChange((e.target as HTMLInputElement).value)}
          class="w-32"
        />
      </div>
    </div>
  {/if}

  <!-- Custom: raw cron input -->
  {#if preset === "custom"}
    <Input
      class="font-mono text-xs"
      placeholder="0 8 * * MON-FRI"
      value={rawCron}
      oninput={(e) => onRawCronChange((e.target as HTMLInputElement).value)}
    />
    <p class="text-caption text-muted-foreground">
      {$t("triggers.cron_syntax_hint")}
    </p>
  {/if}
</div>
