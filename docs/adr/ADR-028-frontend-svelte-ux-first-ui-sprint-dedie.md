# ADR-028 - Frontend Svelte : UX first, UI sprint dédié

**Date :** 2026-03-13
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 14

---

## Contexte

Le Sprint 14 introduit une application desktop (ADR-027). Il faut choisir la stack
frontend et définir la stratégie de design. Deux décisions sont couplées :

1. **Quel framework frontend ?** Le dashboard HTMX existant (Sprint 9) est trop limité
   pour les interactions prévues (HITL compteur en direct, timeline interactive, file
   picker natif, navigation multi-vues).

2. **Quel niveau de polish visuel ?** Le produit est en phase de validation UX - les
   parcours utilisateurs n'ont pas encore été testés sur des utilisateurs réels. Investir
   dans une identité visuelle avant de valider les flux est un risque de bikeshedding.

### Contraintes

- **Budget** : 8-10h/semaine, solo developer. Chaque heure investie en CSS est une
  heure non investie en fonctionnalité.
- **Compétences** : Nidal a une expérience préalable avec Svelte (migration d'un
  workspace IA SaaS), pas avec React récent (hooks, server components).
- **Tauri v2** : support natif de tout framework frontend compilé en assets statiques.
  Pas de contrainte technique sur le choix du framework.
- **Interactions complexes** : HITL real-time (compteur de secondes en direct),
  timeline avec expand/collapse, file picker natif, SSE streaming, toast notifications.

---

## Décision

Nous adoptons **Svelte 5** (runes mode) + **Vite** + **shadcn-svelte** (composants
headless) comme stack frontend.

Pour les Sprints 14 et 15, **aucune customisation visuelle** n'est effectuée :
shadcn-svelte est utilisé avec ses styles par défaut. La patte visuelle Apollia
(tokens de couleur, typographie, illustrations) sera appliquée dans un sprint UI
dédié, une fois les parcours utilisateurs validés sur des utilisateurs réels.

### Stack technique

| Composant | Choix | Justification |
|---|---|---|
| Framework | Svelte 5 (runes) | Réactivité fine-grained, bundle léger, familiarité dev |
| Build tool | Vite 5 | Standard Svelte, HMR rapide, intégration Tauri native |
| Composants | shadcn-svelte | Headless, accessible, surchargeable à 100% via CSS |
| State SSE | Svelte stores (writable/derived) | Natif Svelte, pas de lib externe |
| Navigation | Store `currentRoute` | Simple, pas de dépendance router |
| Theming | CSS custom properties `--apollia-*` | Surchargent les tokens shadcn |

### Design tokens

```css
:root {
  --apollia-bg: hsl(var(--background));
  --apollia-surface: hsl(var(--card));
  --apollia-border: hsl(var(--border));
  --apollia-text: hsl(var(--foreground));
  --apollia-accent: hsl(var(--primary));
  --apollia-danger: hsl(var(--destructive));
  --apollia-success: #16a34a;
  --apollia-warning: #d97706;
}
```

Les valeurs sont celles par défaut de shadcn. Elles seront surchargées dans le
sprint UI dédié sans toucher aux composants.

---

## Alternatives considérées

### Option A - React / Next.js (rejetée)

**Pour :**
- Écosystème massif, beaucoup de composants prêts à l'emploi
- Next.js App Router pour routing avancé
- Large base de développeurs pour contribution future

**Contre :**
- Overhead significatif : React 18+ (hooks, suspense, server components) demande
  un investissement en apprentissage que le budget solo ne permet pas
- Bundle plus lourd que Svelte (~40KB React vs ~5KB Svelte runtime)
- Next.js est un framework server-side - inadapté pour une app Tauri purement client
- Nidal n'a pas d'expérience récente avec React hooks/server components

### Option B - HTMX étendu (rejetée)

**Pour :**
- Dashboard HTMX existant (Sprint 9, STORY-077) - zéro nouvelle dépendance
- Simplicité conceptuelle pour les vues statiques
- Pas de build step frontend

**Contre :**
- Insuffisant pour les interactions complexes du Sprint 14 :
  - HITL compteur en direct (requiert un timer JS côté client)
  - Timeline interactive avec expand/collapse par événement
  - File picker natif Tauri (requiert `@tauri-apps/plugin-dialog`)
  - Navigation multi-vues sans rechargement complet
- Pas de typage (TypeScript absent dans le paradigme HTMX)
- Maintenance difficile : mélange HTML inline dans le binaire Rust
  (`include_str!`) - pas de hot reload, pas de composants réutilisables

### Option C - Svelte + design custom immédiat (rejetée)

**Pour :**
- Identité visuelle dès le départ
- Cohérence de marque

**Contre :**
- Risque de bikeshedding : ajuster les couleurs, typographies, espacements
  avant de savoir si les parcours fonctionnent
- Budget solo : chaque heure CSS = une heure de fonctionnalité en moins
- Les composants shadcn-svelte sont headless - surchargeables à 100% via
  CSS custom properties. Aucun refactoring nécessaire pour appliquer un
  thème plus tard.

### Option retenue - Svelte 5 + shadcn-svelte, UX first

**Pour :**
- Svelte 5 runes : réactivité fine-grained, code concis, bundle léger (~5KB)
- shadcn-svelte : composants accessibles, bien testés, copy-paste dans le projet
  (pas de dépendance npm runtime - les composants sont copiés localement)
- Familiarité de Nidal avec Svelte
- Design tokens CSS prêts à être surchargés sans toucher aux composants
- Vite HMR rapide : feedback immédiat en dev

**Compromis acceptés :**
- L'application aura un look "shadcn par défaut" pendant 2-3 sprints
- Pas d'identité visuelle Apollia avant validation UX
- La navigation est un simple store `currentRoute` - pas de deep linking,
  pas de back/forward navigateur (acceptable pour une app desktop)

---

## Conséquences

**Positives :**
- Stack frontend légère et rapide à développer
- Composants accessibles out-of-the-box (ARIA, keyboard nav)
- Pas de bikeshedding style - focus sur les parcours utilisateurs
- Migration vers un thème custom = uniquement surcharge CSS, zéro refactoring

**Négatives / Compromis :**
- Look "shadcn générique" pendant 2-3 sprints - acceptable en phase de validation
- Svelte 5 runes est récent - documentation moins abondante que Svelte 4
- Pas de deep linking / routing URL dans l'app desktop (simple store)
- Pas de SSR (pas nécessaire pour Tauri, mais limite la réutilisation web)

**Neutres / À surveiller :**
- Si contribution externe significative, l'écosystème React est plus large -
  à réévaluer si le projet passe en mode multi-développeurs
- shadcn-svelte évolue rapidement - surveiller les breaking changes
- Le sprint UI dédié est prévu après validation des parcours sur 3-5
  utilisateurs réels (estimé Sprint 16-17)

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : respecté - les assets Svelte sont compilés
  et embarqués dans le binaire Tauri, zéro CDN, zéro fetch externe
- **Principe #2 - Zéro dépendance externe** : respecté - shadcn-svelte copie
  les composants localement (pas de dépendance runtime npm externe)
- **Principe #8 - CLI humaine, API machine** : étendu - le desktop Svelte
  consomme la même API REST (SSE) et les mêmes handles que le CLI

---

## Liens

- Stories associées : STORY-137 à STORY-142 (Sprint 14), Sprint 15 (vues avancées)
- ADR précédent lié : ADR-027 (processus unique Tauri - définit le cadre d'exécution)
- Référence externe : [shadcn-svelte](https://www.shadcn-svelte.com/)
