<script lang="ts">
  /**
   * AboutCredits - open-source acknowledgements grid.
   *
   * The upstream components embedded in Apollia, with their SPDX license. Names
   * and license identifiers are data (universal), not translated copy; only the
   * public-domain descriptor goes through i18n.
   */
  import { t } from "svelte-i18n";
  import { Cpu, Mic, Cog, AppWindow, Flame, Package, Database, Server } from "lucide-svelte";

  type Glyph = typeof Cpu;

  interface Credit {
    name: string;
    license: string;
    icon: Glyph;
  }

  const publicDomain = $derived($t("settings.about.license_public_domain"));

  const credits = $derived<Credit[]>([
    { name: "llama.cpp", license: "MIT", icon: Cpu },
    { name: "whisper.cpp", license: "MIT", icon: Mic },
    { name: "Tokio", license: "MIT", icon: Cog },
    { name: "Tauri", license: "MIT / Apache-2.0", icon: AppWindow },
    { name: "Svelte", license: "MIT", icon: Flame },
    { name: "PyO3", license: "Apache-2.0", icon: Package },
    { name: "SQLite", license: publicDomain, icon: Database },
    { name: "axum", license: "MIT", icon: Server },
  ]);
</script>

<div class="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4" data-testid="about-credits">
  {#each credits as credit (credit.name)}
    {@const Glyph = credit.icon}
    <div class="flex items-center gap-2.5 rounded-md border border-border/60 bg-surface-1/60 px-2.5 py-2">
      <span class="grid h-6 w-6 shrink-0 place-items-center rounded-md bg-muted text-muted-foreground">
        <Glyph size={14} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <span class="min-w-0">
        <span class="block truncate text-body-xs font-medium text-foreground">{credit.name}</span>
        <span class="block truncate text-overline uppercase text-muted-foreground">{credit.license}</span>
      </span>
    </div>
  {/each}
</div>
