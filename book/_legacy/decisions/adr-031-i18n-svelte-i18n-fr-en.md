# ADR-031 — Stratégie i18n : svelte-i18n avec fichiers JSON FR/EN

**Date :** 2026-03-16
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 16

---

## Contexte

L'interface desktop Apollia OS (Sprint 15) mélange des strings françaises et anglaises codées en dur dans les composants Svelte. Les retours du test utilisateur opérateur Sprint 16 confirment que ce mélange est confus : la sidebar est en anglais, les empty states en français, les notifications en français, les labels de connection en anglais.

Le Sprint 16 introduit un système i18n complet pour :
1. Supprimer toutes les strings codées en dur
2. Supporter français et anglais comme langues de livraison
3. Détecter la langue système au premier lancement
4. Permettre à l'utilisateur de changer la langue dans Settings

### Contraintes

- L'application est Svelte 5 (runes, `$state`, `$derived`) — la librairie i18n doit être compatible
- Les notifications natives Tauri (`tauri-plugin-notification`) utilisent aussi des strings à traduire
- Le projet est solo — la librairie doit être simple à maintenir, pas nécessiter de tooling complexe
- Deux langues seulement pour le MVP (FR + EN) — pas besoin de pluralisation complexe ni de formats ICU

---

## Décision

Nous adoptons **`svelte-i18n`** (v4) avec des fichiers JSON plats (`en.json`, `fr.json`) organisés par namespace.

### Architecture

```
ui/src/lib/i18n/
├── index.ts          ← init(), register(), getLocaleFromNavigator()
├── en.json           ← traductions anglaises (langue par défaut)
└── fr.json           ← traductions françaises
```

### Initialisation

```typescript
import { register, init, getLocaleFromNavigator } from 'svelte-i18n';

register('en', () => import('./en.json'));
register('fr', () => import('./fr.json'));

const savedLocale = localStorage.getItem('apollia-locale');

init({
  fallbackLocale: 'en',
  initialLocale: savedLocale || getLocaleFromNavigator(),
});
```

### Usage dans les composants

```svelte
<script>
  import { t } from 'svelte-i18n';
</script>

<h1>{$t('agents.title')}</h1>
<p>{$t('agents.empty')}</p>
<Button>{$t('agents.register')}</Button>
```

### Namespaces (11 groupes)

| Namespace | Contenu |
|---|---|
| `nav` | Labels sidebar, groupes, connection status |
| `agents` | Page Agents, AgentCard, AgentLogs |
| `tasks` | Page Tasks, TaskList, TaskDetail, TaskTimeline |
| `approvals` | Page Approvals, ApprovalCard, ApprovalHistory |
| `llm` | Page LLM, LlmBackendCard, LlmStats |
| `triggers` | Page Triggers, TriggerRow, TriggerLogs |
| `pipelines` | Page Pipelines, PipelineRunCard, PipelineRunDetail |
| `memory` | Page Memory, NamespaceSelector, MemorySearch, MemoryTable |
| `notifications` | Page Notifications, channels, logs |
| `settings` | Page Settings, sections, advanced |
| `onboarding` | Wizard, steps, checks |
| `common` | Boutons, états, erreurs, formatage, timestamps relatifs |
| `dashboard` | Page Dashboard (Sprint 16) |

---

## Alternatives considérées

### Option A — `@inlang/paraglide-sveltekit` (rejetée)

**Pour :**
- Compilation statique des traductions — meilleure performance (tree-shaking, pas de runtime)
- Typage fort : les clés de traduction sont vérifiées à la compilation
- Écosystème Inlang (editor visuel, CI/CD lint, extraction automatique)

**Contre :**
- Conçu pour SvelteKit avec routing filesystem — Apollia Desktop utilise Svelte 5 pur avec routage par store (`currentRoute`), pas SvelteKit
- La configuration nécessite un `project.inlang/` avec des fichiers de config spécifiques — overhead pour 2 langues
- Le typage fort est un avantage quand il y a beaucoup de langues/contributeurs — pour 2 langues maintenues par 1 personne, c'est de l'over-engineering
- Moins mature que svelte-i18n pour du Svelte pur (hors SvelteKit)

### Option B — Solution custom (writable store + JSON) (rejetée)

**Pour :**
- Zéro dépendance externe
- Contrôle total sur l'API

**Contre :**
- Réinventer la roue : interpolation de variables (`{name}`), pluralisation basique, détection locale, formatage dates/nombres
- `svelte-i18n` fait tout ça en ~15KB — pas justifié de réécrire
- Maintenance sur le long terme quand des langues seront ajoutées

### Option retenue — `svelte-i18n` v4

**Pour :**
- Librairie la plus utilisée dans l'écosystème Svelte (3M+ downloads/mois)
- Compatible Svelte 5 (utilise des stores Svelte standards)
- API simple : `$t('key')`, `$t('key', { values: { name: 'X' } })`
- Détection locale native (`getLocaleFromNavigator()`)
- Lazy loading des traductions via `register()` + `import()`
- Interpolation, pluralisation, formatage dates/nombres inclus
- Petite taille (~15KB gzipped)

**Compromis acceptés :**
- Pas de vérification des clés à la compilation (risque de clé manquante en runtime — mitigé par un script de vérification de parité FR/EN)
- Pas de tree-shaking des traductions inutilisées (acceptable pour 2 fichiers JSON de ~5KB chacun)
- Dépendance runtime sur svelte-i18n (acceptable — librairie stable et maintenue)

---

## Conséquences

**Positives :**
- Interface 100% traduite FR et EN — plus de mélange incohérent
- Détection automatique de la langue système au premier lancement
- Persistance du choix utilisateur dans localStorage
- Les futures langues (DE, ES, etc.) se résument à ajouter un fichier JSON
- Les notifications natives utilisent aussi le système i18n

**Négatives / Compromis :**
- Migration massive : toutes les strings en dur des 10 routes + 29 composants doivent être extraites — risque de régression si une string est oubliée
- Les clés de traduction ne sont pas typées — une typo dans `$t('agents.titel')` ne sera détectée qu'à l'exécution (fallback silencieux vers la clé brute)
- ~15KB de bundle supplémentaire

**Neutres / À surveiller :**
- Script de vérification de parité FR/EN à inclure dans le CI/build
- Si Apollia OS migre vers SvelteKit un jour, reconsidérer Paraglide à ce moment
- Le formatage des dates/nombres utilise `Intl` (natif navigateur) via svelte-i18n — pas de polyfill nécessaire dans Tauri WebView

---

## Principes architecturaux impactés

- **Principe #8 — CLI humaine, API machine** : étendu — l'UI desktop s'adapte à la langue de l'utilisateur. Le CLI reste en anglais (pas d'i18n CLI pour le MVP).

---

## Liens

- Story associée : STORY-157
- ADR précédent lié : ADR-028 (frontend Svelte UX first — l'i18n fait partie de l'UX)
