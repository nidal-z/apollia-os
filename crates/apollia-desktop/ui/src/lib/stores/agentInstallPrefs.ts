/**
 * Préférences d'installation des packages d'agents.
 *
 * Pour l'instant un seul champ : `autoInstallPythonDeps`.
 * Quand ce flag est vrai, la modale d'install passe `depsConfirmed=true`
 * automatiquement au backend, sans demander de confirmation explicite à
 * l'utilisateur pour chaque package qui déclare des dépendances pip.
 *
 * Par défaut : `false` (sécurité par défaut - toute install qui touche
 * pip doit être explicitement consentie).
 *
 * Persisté en localStorage (pattern aligné sur `mode.ts`). `apollia.toml`
 * reste read-only depuis l'UI, donc on évite
 * d'y écrire ce genre de préférence.
 */
import { writable } from "svelte/store";

const STORAGE_KEY = "apollia.agent-install-prefs";

export interface AgentInstallPrefs {
  /** Installer automatiquement les dépendances Python sans dialog de confirmation. */
  autoInstallPythonDeps: boolean;
}

const DEFAULT_PREFS: AgentInstallPrefs = {
  autoInstallPythonDeps: false,
};

function loadPersisted(): AgentInstallPrefs {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return DEFAULT_PREFS;
    const parsed = JSON.parse(stored);
    return {
      autoInstallPythonDeps:
        typeof parsed?.autoInstallPythonDeps === "boolean"
          ? parsed.autoInstallPythonDeps
          : DEFAULT_PREFS.autoInstallPythonDeps,
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

/**
 * Préférences d'install. Mises à jour persistées en localStorage.
 * Lire la valeur courante via `get(agentInstallPrefs)` (svelte/store).
 */
export const agentInstallPrefs = writable<AgentInstallPrefs>(loadPersisted());

agentInstallPrefs.subscribe((value) => {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // localStorage indisponible - ignorer silencieusement.
  }
});

/** Helper : flip un champ booléen et persister. */
export function setAutoInstallPythonDeps(enabled: boolean): void {
  agentInstallPrefs.update((prefs) => ({ ...prefs, autoInstallPythonDeps: enabled }));
}
