<script lang="ts">
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";
  import { TimePicker } from "$lib/components/ui/date-picker";
  import { stripSecondsField } from "$lib/automations/humanize";

  interface Props {
    value: string;
    onchange: (expr: string) => void;
  }

  let { value, onchange }: Props = $props();

  type Preset = "15m" | "30m" | "hourly" | "daily" | "weekly" | "custom";

  // cron day-of-week: 0=Sun,1=Mon,...,6=Sat - displayed as Mon-Sun
  const DAYS_LABEL = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  const DAYS_CRON = [1, 2, 3, 4, 5, 6, 0]; // Mon→1, ... Sun→0

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

  // The trigger engine evaluates cron expressions in UTC, but the pickers show
  // local wall-clock time. Convert local <-> UTC so "daily at 08:00" fires at
  // 08:00 local, shifting the weekday when the conversion crosses midnight. DST
  // is approximated with the current offset: a recurring schedule cannot encode
  // a per-occurrence offset.
  function tzOffsetMinutes(): number {
    // getTimezoneOffset returns minutes to add to local time to reach UTC.
    return new Date().getTimezoneOffset();
  }

  function shiftMinutes(hh: number, mm: number, delta: number): { hh: number; mm: number; dayDelta: number } {
    let total = hh * 60 + mm + delta;
    let dayDelta = 0;
    while (total < 0) { total += 1440; dayDelta -= 1; }
    while (total >= 1440) { total -= 1440; dayDelta += 1; }
    return { hh: Math.floor(total / 60), mm: total % 60, dayDelta };
  }

  const localToUtc = (hh: number, mm: number) => shiftMinutes(hh, mm, tzOffsetMinutes());
  const utcToLocal = (hh: number, mm: number) => shiftMinutes(hh, mm, -tzOffsetMinutes());
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
      const l = utcToLocal(hour, min);
      dailyTime = `${pad(l.hh)}:${pad(l.mm)}`;
    } else if (p === "weekly") {
      const parts = fiveField.split(" ");
      const l = utcToLocal(Number(parts[1]), Number(parts[0]));
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

  function buildExpr(p: Preset, dTime: string, wTime: string, wDays: boolean[], raw: string): string {
    switch (p) {
      case "15m": return "*/15 * * * *";
      case "30m": return "*/30 * * * *";
      case "hourly": return "0 * * * *";
      case "daily": {
        const [hh, mm] = dTime.split(":").map(Number);
        const u = localToUtc(hh, mm);
        return `${u.mm} ${u.hh} * * *`;
      }
      case "weekly": {
        const [hh, mm] = wTime.split(":").map(Number);
        const u = localToUtc(hh, mm);
        const activeDays =
          DAYS_CRON.filter((_, i) => wDays[i])
            .map(d => ((d + u.dayDelta) % 7 + 7) % 7)
            .join(",") || "0";
        return `${u.mm} ${u.hh} * * ${activeDays}`;
      }
      case "custom": return raw;
    }
  }

  function selectPreset(p: Preset) {
    preset = p;
    onchange(buildExpr(p, dailyTime, weeklyTime, weeklyDays, rawCron));
  }

  function onDailyTimeChange(t: string) {
    dailyTime = t;
    if (preset === "daily") onchange(buildExpr("daily", t, weeklyTime, weeklyDays, rawCron));
  }

  function onWeeklyTimeChange(t: string) {
    weeklyTime = t;
    if (preset === "weekly") onchange(buildExpr("weekly", dailyTime, t, weeklyDays, rawCron));
  }

  function toggleDay(i: number) {
    weeklyDays[i] = !weeklyDays[i];
    if (preset === "weekly") onchange(buildExpr("weekly", dailyTime, weeklyTime, weeklyDays, rawCron));
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
      Standard cron syntax: min hour day month weekday
    </p>
  {/if}
</div>
