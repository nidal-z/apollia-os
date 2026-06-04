# Breakpoints responsive - Apollia Desktop

> Source de vérité pour tous les breakpoints utilisés dans `apollia-desktop/ui`.
> Adossé à l'ADR-021.
> Toute déviation (breakpoint custom, largeur en px dans le code) est un bug à corriger.

---

## Tokens canoniques

Les breakpoints sont déclarés dans [`tailwind.config.ts`](../../../tailwind.config.ts) et
correspondent un à un aux préfixes Tailwind standard, enrichis d'un `xs` dédié au
« operator mobile ».

| Token  | `min-width` | Cible principale                                  |
|--------|-------------|---------------------------------------------------|
| `xs`   | `375px`     | **Operator mobile** (iPhone SE, seuil minimum supporté) |
| `sm`   | `640px`     | Tablette portrait, fenêtre desktop étroite        |
| `md`   | `768px`     | Tablette paysage, split-screen desktop 2 colonnes |
| `lg`   | `1024px`    | Laptop standard - cible de référence builder      |
| `xl`   | `1280px`    | Desktop large                                     |
| `2xl`  | `1536px`    | Desktop ultra-wide, dashboards pleins             |

Les valeurs 640/768/1024/1280/1536 sont celles par défaut de Tailwind.
L'ajout explicite de `xs: 375px` formalise le contrat « on supporte jusqu'à 375 px »
et rend cette contrainte testable et observable dans les outils (navigateurs,
devtools Tailwind).

## Seuil « operator mobile »

La persona **operator** peut consulter son agent depuis un mobile (check rapide de
tâche, approbation HITL, lecture de notif). Le **seuil minimum** est défini à
**375 px** (iPhone SE). En dessous, le rendu n'est pas garanti - on assume une
dégradation acceptable.

La persona **builder** ne descend pas sous `md` (768 px) : la profondeur
d'observabilité exige de la place.

## Règles d'usage

1. **Toujours préfixer les utilitaires responsive avec les tokens Tailwind.**
   ```html
   <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3">…</div>
   ```
2. **Aucune valeur brute en `px` dans le code Svelte** (classes `w-[420px]`,
   `max-w-[600px]`, `@media (max-width: 560px)`, etc.) - sauf cas explicitement
   justifié par un commentaire `// @responsive-exception: raison` relié à un ADR.
3. **Conteneurs de page** : toutes les routes de `src/routes/**.svelte` doivent
   utiliser le pattern canonique :
   ```html
   <div class="mx-auto w-full max-w-6xl px-4 sm:px-6 lg:px-8">…</div>
   ```
4. **Grids dense** : démarrer mobile-first (`grid-cols-1`), densifier via `sm:`,
   `lg:`, `xl:`. Une grille `grid-cols-5` hardcodée est un bug.
5. **Tables** : jamais une `<table>` nue. Envelopper dans
   [`<ResponsiveTable>`](../components/common/ResponsiveTable.svelte) qui gère
   `overflow-x-auto` et un slot `card` mobile.
6. **Dialog / Sheet** : utiliser `w-full sm:max-w-[…]` (voir
   `lib/components/ui/dialog/Dialog.svelte` et `.../sheet/Sheet.svelte`).
7. **Typographie de conteneur** : préférer `clamp()` CSS pour les titres qui
   doivent s'adapter sans `@media` (ex: héros onboarding, titres projets).

## Détection des violations

Un lint visuel doit être passé aux 4 largeurs critiques avant merge :

| Largeur    | Contexte                    |
|------------|-----------------------------|
| `375 px`   | operator mobile (limite basse) |
| `768 px`   | charnière `sm → md`         |
| `1024 px`  | charnière `md → lg` - laptop builder |
| `1440 px`  | desktop confort             |

Grep de garde (devrait retourner 0 occurrences en dehors des exceptions) :

```bash
rg -n "w-\[[0-9]+px\]|max-w-\[[0-9]+px\]|min-w-\[[0-9]+px\]" src/
rg -n "@media\s*\([^)]*max-width" src/
rg -n "window\.innerWidth|matchMedia" src/
```
