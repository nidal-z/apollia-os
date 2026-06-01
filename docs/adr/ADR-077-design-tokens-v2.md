# ADR-077 - Design tokens v2 : elevation, warmth dark, rim lights

**Date :** 2026-04-19
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 42 - Frontend redressement (US-SP42-004)

---

## Contexte

L'audit design Sprint 42 (findings F.19, F.20, F.33, F.34, F.41, F.55, F.56,
F.74, F.75, A.1.10, A.3.1) a relevé que le système de tokens hérité de
Sprint 34 - une palette HSL plate + `box-shadow` hardcodés disséminés dans
les composants - produisait :

1. **Surfaces plates.** Cards, modals et CTAs utilisaient un unique
   `box-shadow` fixé à `0 2px 8px…`, sans hiérarchie d'élévation. Rien ne
   distinguait visuellement une ligne de table d'un menu flottant ou d'un
   modal hero (F.19, F.20).
2. **Dark "black on black".** `.glass-inset` en dark : `rgba(255, 255, 255,
   0.05)` - invisible (F.55). Le fond `--background: 240 12% 9%` est neutre
   froid, ce qui contraste mal avec l'identité blue/violet (F.33, F.34).
3. **Identité primary diluée.** Cinq CTAs clés (Dashboard empty state, Chat
   send, Agent start, Wizard next, Trigger fire) utilisaient `bg-primary`
   générique Tailwind sans promotion visuelle - indiscernables d'un bouton
   secondary (F.41, A.1.10).
4. **Box-shadows dupliqués.** Grep `box-shadow:` dans `ui/src/**/*.svelte`
   retournait **48 occurrences hardcodées** (48 valeurs indépendantes). Toute
   évolution de la marque exigeait 48 éditions et aucune garantie de
   cohérence.
5. **Backdrop modal uniforme** - même `rgba(0,0,0,.45)` light et dark ;
   invisible en dark warm, quasi noir plein en light (F.56).
6. **Glass-border identique** en light et dark - perdait tout contraste dans
   l'un ou l'autre mode (F.74, F.75).

La décision doit être prise maintenant : Sprint 42 a pour vocation de
redresser le frontend avant le lancement public. Sans système de tokens
cohérent, chaque US Sprint 42 qui ajoute un composant continue de dupliquer
l'ancien système, creusant la dette.

---

## Décision

Introduire **Design tokens v2** dans `crates/apollia-desktop/ui/src/app.css`
comme **source unique de vérité** pour toutes les ombres, surfaces, et
expositions du primary. La règle devient : **aucun `box-shadow:` ne peut
être hardcodé dans un fichier `.svelte`** - toute ombre passe par `var(...)`.

Cinq décisions précises :

### 1. Échelle d'élévation à 5 niveaux avec rim light

`--shadow-elev-0` à `--shadow-elev-4`. Chaque niveau combine :
- 2 layers `box-shadow` externes (près + loin) pour la profondeur.
- 1 layer `inset 0 1px 0` (rim light) pour simuler un reflet bord supérieur -
  crucial pour la sensation "matériau" (F.19, F.20).

Le rim est **warm-tinted** :
- Light : `rgba(255, 252, 240, .5)` à `.8` (cream-white).
- Dark : `hsl(32 30% 70% / .10)` à `.18` (bronze-white) - résout le
  "black on black" en dark (F.55).

Chaque niveau redéclaré en `.dark` avec opacités adaptées (niveau 4 dark
porte une teinte primary `rgba(52, 53, 245, .35)` comme glow secondaire).

### 2. Surfaces warm dark

La palette dark est décalée de neutre (`240° hue`) vers brun warm
(`28° hue`) pour produire un contraste tangible avec le primary
blue/violet (F.33, F.34). Cible mesurable : `--background: hsl(28 8% 9%)`
soit un delta-E perceptible vs `hsl(240 12% 9%)`.

Trois niveaux de surface `--surface-1`, `--surface-2`, `--surface-3`
(élevé → recessed) s'ajoutent à `--background` / `--muted` pour permettre
des hiérarchies de profondeur (A.3.1).

### 3. Primary utilities explicites

Trois classes CSS - également exposées comme variants Button - :
- `.bg-primary-solid` : fond plein + `shadow-primary-sm/md` au hover.
- `.bg-primary-gradient` : gradient 135° primary→secondary + élévation.
- `.border-primary-subtle` : bordure primary `25 %` → `50 %` au hover.

Appliquées sur les 5 CTAs clés (F.41, A.1.10). Le variant `default` du
Button reste mais est déprécié au profit de `primary-solid` pour les
actions promues.

Tailwind expose les primary-tinted shadows : `shadow-primary-sm/md/lg/xl`.

### 4. Glass borders différenciés

`--glass-border-light` / `--glass-border-light-hover` / `--glass-border-dark`
/ `--glass-border-dark-hover`. La classe `.glass-border` bascule
automatiquement au `:hover` (F.74, F.75).

### 5. Backdrop spécifique mode

- Light : `rgba(0, 0, 0, .40)` + `backdrop-blur-md`.
- Dark : `hsl(28 10% 5% / .70)` + `backdrop-blur-md` (warm-tinted, cohérent
  avec la palette).

Classe `.app-backdrop` (et `.app-backdrop-subtle` pour les popovers).

---

## Alternatives envisagées

1. **Étendre Tailwind `shadow-*` natif.** Rejetée : Tailwind ne supporte pas
   `inset` rim lights ergonomiquement, et le système n'aurait pas été
   exposable côté TypeScript sans duplication.
2. **Variables JS pilotées par un store theme.** Rejetée : un store implique
   des re-renders et empêche l'utilisation dans les `@keyframes`. Les CSS
   vars permettent de changer de mode via `.dark` sans JS.
3. **Conserver `box-shadow` hardcodés, autoriser mais documenter.** Rejetée :
   violait Principe #8 (API machine = grep-friendly) et ne scalait pas. La
   refacto des 48 occurrences est massive mais ponctuelle.
4. **Recréer un système type Material Design `dp`.** Rejetée : over-engineering
   pour une app Tauri single-window ; `elev-0..4` couvre 100 % des besoins
   observés dans le code existant.

---

## Conséquences

### Positives

- Cohérence visuelle immédiate sur l'ensemble de l'app. Les 48 valeurs
  hardcodées deviennent 9 tokens réutilisés.
- Dark mode mesurable-ment warm : `--background` passe de `240° 12°/9%` à
  `28°/8°/9%`.
- CTAs portant l'identité visible au premier coup d'œil (primary-solid +
  primary-gradient).
- Évolution future de la marque = édition de `app.css` uniquement.
- Tests de non-régression visuels triviaux : grep `box-shadow:` qui ne
  renvoie que des `var(...)`.
- Documentation centralisée : `docs/design-system.md` + route `#design` en
  DEV pour la preview.

### Négatives

- Refactor invasif : 15 fichiers `.svelte` (essentiellement `components/onboarding/*`)
  ont leurs shadows reconvertis vers tokens. Régressions pixel-perfect
  possibles sur les animations `logo-pulse`, `rail-pulse` - mitigation :
  tokens `--logo-shadow-rest/active` et `--rail-dot-rest/pulse` scoped à
  leur composant pour préserver l'intention.
- Variant `default` du Button ambigu vs `primary-solid` - les deux ciblent
  le primary. Décision : laisser `default` pour rétro-compat sur les boutons
  secondaires-semi-promus, la doc indique quand préférer `primary-solid`.
- Pas de couverture e2e automatique du rendu visuel : la route `#design`
  doit être vérifiée manuellement à chaque évolution des tokens.

### Neutres

- Les fichiers `OnboardingWelcome.svelte`, `TourProgressRail.svelte` gardent
  des CSS vars locales (`--logo-shadow-rest`, `--rail-dot-rest`) - c'est
  accepté car ces animations sont spécifiques et ne gagnent pas à être
  hissées en tokens globaux.
- `apollia-desktop/ui/src/lib/design/tokens.ts` expose les tokens en
  TypeScript ; le JS n'est pas utilisé à ce jour mais prépare la
  consommation par un éventuel theme builder futur.

---

## Lien story

`docs/internal/STORIES/sprint-42/story-004-design-tokens.md`.
