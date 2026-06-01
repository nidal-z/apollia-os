# ADR-076 - Internationalisation du frontend desktop (svelte-i18n)

**Date :** 2026-04-19
**Statut :** Accepté
**Sprint :** 42 (redressement frontend)

---

## Contexte

Le frontend `apollia-desktop` (Svelte 5 + Tauri v2) est historiquement mixte FR/EN : chaînes hardcodées dans les `.svelte`, labels français codés en dur dans des composants anglais, aucune convention pour les `aria-label` icon-only. L'audit Sprint 42 a identifié plusieurs findings liés à ce chaos :

- **A.4.1** : "Workspace" en anglais dans `ProjectDetail` alors que l'interface opérateur est en français par défaut.
- **B.11** : "Thinking..." / "Thought" hardcodés dans `StreamingText`.
- **B.34** : "Libre" (legend mémoire) déjà présent côté FR mais clé inexistante côté EN.
- **D.31** : `PlanCacheStats` entièrement en français hardcodé.
- **E.48** : `aria-label` icon-only ("Close", "Dismiss", "Actions", "Microphone"…) en clair dans le markup - aucun parcours clavier FR.
- **E.65** : convention de capitalisation des boutons non alignée (mélange "Title Case", "Sentence case", UPPERCASE).

Le runtime livre déjà `svelte-i18n` (4.0.1) avec un catalogue `en.json`/`fr.json` à ~1700 clés, un switch de langue dans Settings et un test de parité `i18n-tools.test.ts`. L'outil est en place, mais l'usage est incomplet et la convention de nommage n'est pas documentée.

La décision doit être prise maintenant parce que le Sprint 42 vise un redressement frontend « plus transparent que Claude.ai », ce qui impose un socle FR/EN propre avant l'ajout des nouvelles vues Operator/Builder.

## Décision

**Nous adoptons `svelte-i18n` comme unique mécanisme d'internationalisation du frontend, avec `fr` comme locale par défaut, `en` comme fallback, et un catalogue JSON source-de-vérité doublé d'un index TypeScript par zone.**

Règles opérationnelles :

1. **Source de vérité = JSON.** `src/lib/i18n/en.json` et `src/lib/i18n/fr.json` contiennent toutes les chaînes. Toute nouvelle string passe d'abord par les deux JSON, jamais hardcodée dans un `.svelte`.
2. **Index typé par zone.** `src/lib/i18n/strings/*.ts` expose des constantes typées (`CHAT_KEYS`, `OBSERVABILITY_KEYS`…) pour les clés consommées programmatiquement (e.g. `EmptyState` variants, colonnes de table). Utile pour grep, pas obligatoire pour chaque `$t("...")` inline.
3. **Convention de clés.** `zone.sous_zone.contexte` en `snake_case`. Exemples : `observability.plan_cache.dialog_title`, `chat.plan_alternatives.choose_plan_a`, `a11y.close`. Pour les `aria-label` icon-only : toujours sous `a11y.<nom>`.
4. **Convention de capitalisation (E.65).**
   - **FR** : sentence case partout (premier mot capitalisé, le reste en minuscule). Ex : « Créer un projet ».
   - **EN** : sentence case par défaut (« Create a project ») - aligné sur la convention Material / Atlassian moderne. Les badges système restent UPPERCASE (`DEFAULT`, `ACTIVE`). Les marques et identifiants techniques conservent leur casse (`OpenAI`, `llama-cpp`).
   - Un mélange historique subsiste dans le catalogue ; la normalisation complète est déférée à une story dédiée (voir *À surveiller*).
5. **Détection de locale.** À l'initialisation : `localStorage.apollia-locale` si défini, sinon `getLocaleFromNavigator()` si supporté (`fr` ou `en`), sinon `fr`. L'utilisateur peut basculer depuis Settings, choix persisté.
6. **Whitelist brand.** Les marques et identifiants techniques ne sont pas traduits : `Apollia`, `Apollia OS`, `MCP`, `OpenAI`, `Mistral`, `Anthropic`, `Ollama`, `Metal`, `CUDA`, `GPU`. Les placeholders d'exemple de type `qwen3-0.6b-q8_0` ou `local-code` non plus.
7. **Vues design-system exclues.** `src/routes/Design*.svelte` (showcase tokens/motion/empty-states) sont des pages dev-only et échappent à la traduction.

Un script `scripts/audit-i18n.mjs` (exposé en `npm run audit:i18n`) grep les chaînes hardcodées restantes et exit ≠ 0 si l'inventaire n'est pas vide. Le test `i18n-locale-switch.test.ts` vérifie que basculer la locale bascule bien les strings sur les findings clés du sprint.

## Alternatives considérées

### Option A - `paraglide-js` (rejetée)
**Pour :** bundle optimal (tree-shaking), typage fort natif, syntaxe ICU MessageFormat complète, DX plus moderne.
**Contre :** déjà 1700 clés en place avec `svelte-i18n` utilisées dans ~150 composants. Migration = risque net sans gain visible pour l'utilisateur dans le Sprint 42. `paraglide` reste une option pour une future réécriture majeure.

### Option B - Pas de bibliothèque, `JSON + get()` maison (rejetée)
**Pour :** zéro dépendance runtime, contrôle total.
**Contre :** on réécrirait l'interpolation `{param}`, la pluralisation ICU, le store réactif Svelte. Sous-investissement déjà validé par `svelte-i18n`.

### Option retenue - `svelte-i18n` (status quo enrichi)
**Pour :** déjà intégré, store `$t` réactif, interpolation `{count}` et ICU `{n, plural, …}` supportés, tests existants, langue changeable sans reload.
**Compromis acceptés :** bundle légèrement plus gros que paraglide ; conventions à documenter manuellement (cet ADR).

## Conséquences

**Positives :**
- Un opérateur FR obtient une interface 100 % française après `npm run audit:i18n` vert.
- Les `aria-label` icon-only lisent en français sous VoiceOver/NVDA avec locale FR.
- Toute nouvelle vue Sprint 42+ doit passer par les JSON, le script CI empêche la régression.
- Les index TS `strings/*.ts` donnent une cartographie grep-friendly des clés par zone.

**Négatives / Compromis :**
- Catalogue JSON volumineux (~1700 clés × 2 locales). Le chargement init reste sync via imports statiques - acceptable sur desktop.
- La convention de capitalisation n'est pas enforced mécaniquement ; repose sur la revue de code et l'ADR.
- Le test `i18n-locale-switch` couvre un échantillon ciblé, pas l'exhaustivité du catalogue.

**À surveiller :**
- **Normalisation capitalisation** : prévoir une story dédiée pour passer les ~1700 clés EN au sentence case uniforme avant la release publique.
- **Tailles de bundle** : si le catalogue double (ajout de locales ES/DE/etc.), migrer vers lazy-loading par locale (`register("es", () => import(...))` est déjà compatible).
- **Pluralisation ICU** : actuellement utilisée seulement dans `chat.message_count` ; à généraliser quand de nouveaux compteurs apparaissent.
- **Drift de clés** : si `paraglide-js` devient une priorité, la migration est facilitée par le fait que les keys sont déjà typées dans `strings/*.ts`.

## Principes architecturaux impactés

- **Principe #8 - CLI humaine, API machine** : étendu au frontend - l'UI est humaine (FR par défaut), mais les `data-testid` restent machine-readable et non-traduits.
- **Principe #1 - Local-first** : la locale est persistée en `localStorage`, zéro appel réseau.

## Liens

- Story associée : US-SP42-008
- Spec sprint : `docs/internal/STORIES/sprint-42/`
- Audit a11y précédent : US-SP42-007 (ADR non créé, script `audit-a11y.mjs`)
- Catalogue : `crates/apollia-desktop/ui/src/lib/i18n/`
- Audit : `crates/apollia-desktop/ui/scripts/audit-i18n.mjs` (`npm run audit:i18n`)
- Test locale-switch : `crates/apollia-desktop/ui/src/lib/i18n/i18n-locale-switch.test.ts`
