# Guide Figma-first, assisté par Claude

> Méthode de travail pour Apollia OS. Objectif: aucune UI n'est codée avant
> d'exister et d'être validée dans Figma. Figma devient la spec exécutable, le
> code n'est plus qu'une transcription fidèle. Résultat: des implémentations
> 100% propres et 100% cohérentes avec l'UI et les composants existants.

Fichier Figma de référence: **"Apollia OS Design System"**
`https://www.figma.com/design/2TLZ2uqIOweX14eP4VGXHq`
Pages: 🎨 Tokens · 🧩 Primitives · 🧱 Features (×5) · 📐 Templates · 🔍 Audit.

---

## 1. L'idée en une phrase

On conçoit dans Figma (assisté par Claude), tu valides le visuel, puis seulement
on code. Le frame Figma validé est la définition de "prêt à coder". Le code
réutilise les mêmes tokens et les mêmes composants, donc il ne peut pas diverger.

## 2. Pourquoi changer de méthode

- **Cohérence native**: tout part des mêmes variables (couleurs Light/Dark,
  radius, spacing, ombres) et des mêmes composants. Plus de CSS ad hoc qui dérive.
- **Revue avant le code**: on tranche le visuel sur un screenshot, pas après
  trois allers-retours d'implémentation.
- **Réutilisation**: on assemble des instances de la librairie, pas des formes
  libres. Un changement de composant se propage partout.
- **Traçabilité**: `figma/MAPPING.md` relie chaque node Figma à son `.svelte`.

## 3. Le contrat (à appliquer systématiquement)

**Definition of Ready (avant d'écrire une ligne de code):**
- un frame Figma existe pour l'écran ou le composant,
- il est composé d'instances de la librairie (pas de rectangles dessinés à la main),
- toutes les valeurs sont bindées à des variables (aucune couleur/space en dur),
- il rend correctement en mode Dark **et** Light (on flippe le mode pour vérifier),
- tu l'as validé.

**Definition of Done (après le code):**
- le rendu Svelte est cohérent avec le frame (mêmes proportions, mêmes états),
- le code utilise les tokens (`var(--...)` / classes Tailwind mappées), zéro
  valeur en dur quand un token existe (voir `crates/apollia-desktop/ui/AGENTS.md`),
- le composant réutilisé est bien celui de la librairie, pas une recopie.

## 4. Les pièces du système

| Pièce | Rôle | Source de vérité |
|---|---|---|
| Variables Figma (Color Light/Dark, Radius, Spacing) | tokens partagés | `src/app.css` (`:root` + `.dark`) + `tailwind.config.ts` |
| Text styles / Effect styles | typo + ombres | `tailwind.config.ts` `fontSize` / `src/app.css` `--shadow-*` |
| Page 🧩 Primitives | briques réutilisables | `src/lib/components/ui/**` |
| Pages 🧱 Features | composants métier | `src/lib/components/**` |
| Page 📐 Templates | écrans assemblés en instances | `src/routes/**` |
| Page 🔍 Audit + `figma/MAPPING.md` | carte node-id <-> source + couverture | (générés) |

## 5. Workflow Figma2Code (le flux principal)

Six étapes. À chaque étape, le prompt à donner à Claude est dans la section 8.

1. **Cadrer.** Tu décris la feature (user story, écrans, états attendus:
   vide / chargement / erreur / succès). Claude reformule et liste les écrans.
2. **Vérifier la couverture.** Claude lit `figma/MAPPING.md` et la page Audit:
   les composants nécessaires existent-ils déjà? Si un composant manque, on passe
   d'abord par le sous-flux "nouveau composant" (section 6).
3. **Composer dans Figma.** Claude assemble l'écran à partir d'**instances** de la
   librairie (via `use_figma`), sur la page 📐 Templates, en mode Dark.
4. **Revue visuelle.** Claude fournit un screenshot. Tu regardes, tu flippes
   Light/Dark, tu demandes des ajustements. On itère jusqu'à ce que tu valides.
5. **Geler.** Le frame validé devient la spec. On note son node-id.
6. **Implémenter.** Claude lit le frame (`get_design_context` sur le node-id),
   puis écrit le Svelte en réutilisant les vrais composants et les tokens, et
   vérifie le rendu contre le screenshot du frame.

Règle d'or: on ne démarre l'étape 6 qu'après ta validation à l'étape 4.

## 6. Sous-flux "nouveau composant" (primitive ou feature absente)

Un composant naît dans Figma avant le code.

1. Claude écrit la spec depuis l'intention (variantes, tailles, états).
2. Claude crée le variant set dans Figma, bindé aux variables, Light/Dark,
   nommé en PascalCase comme le futur `.svelte`.
3. Tu valides (screenshot).
4. On enregistre: node-id dans `figma/MAPPING.md` + un `figma/code-connect/<nom>.figma.ts`.
5. **Ensuite seulement** on code le `.svelte` (qui réutilise tokens + primitives).

## 7. Workflow Code2Figma (le flux inverse, ponctuel)

À utiliser quand le code a bougé avant le design: prototypage rapide en code,
refacto, ou dette existante.

1. On code ou on ajuste le composant.
2. Claude relit le `.svelte` et met à jour le variant set Figma correspondant.
3. On met à jour `figma/MAPPING.md` (+ le `.figma.ts` si les variantes ont changé).
4. Pour capturer un écran web réel d'un coup: `generate_figma_design` fait une
   capture, puis `use_figma` le reconstruit avec les composants de la librairie.

Le Code2Figma reste l'exception. Le défaut, c'est Figma2Code.

## 8. Boîte à prompts (copier-coller à Claude)

- **Cadrer + couverture:** "Pour la feature <X>, liste les écrans et états, puis
  vérifie dans `figma/MAPPING.md` et la page Audit quels composants existent
  déjà et lesquels manquent."
- **Nouveau composant:** "Le composant <Nom> n'existe pas. Crée-le d'abord dans
  Figma (variant set, bindé aux variables, Light/Dark), screenshot pour revue,
  puis ajoute-le à `MAPPING.md` et un `.figma.ts`. Ne code rien tant que je n'ai
  pas validé."
- **Composer un écran:** "Compose l'écran <route> sur la page Templates à partir
  d'instances de la librairie (shell Sidebar + Topbar inclus), en mode Dark.
  Donne-moi un screenshot."
- **Itérer:** "Sur le frame node-id=<X>: <ajustements>. Re-screenshot."
- **Vérifier le thème:** "Bascule ce frame en Light et montre-moi le rendu."
- **Implémenter:** "Implémente le frame node-id=<X> en Svelte en réutilisant
  <composants> et les tokens (aucune valeur en dur). Puis vérifie le rendu réel
  contre le screenshot du frame."
- **Sync token:** "`--primary` a changé dans `app.css`. Régénère la variable
  Figma (Light + Dark) et liste les composants impactés."
- **Sync variantes:** "J'ai ajouté une variante à <Composant>. Mets à jour le
  variant set Figma, `MAPPING.md` et le `.figma.ts`."

## 9. Garder le sync (3 règles)

1. **Token modifié** dans `app.css`/`tailwind.config.ts` -> régénérer la variable
   Figma (modes Light/Dark), tout ce qui est bindé se met à jour.
2. **Variantes modifiées** -> mettre à jour le set Figma + `.figma.ts` + `MAPPING.md`.
3. **Nouveau composant** -> Figma d'abord, puis node-id dans `MAPPING.md`, puis
   `.figma.ts`, puis code.

Plus tard: activer **Code Connect en live** (Dev Mode affiche l'import Svelte
réel sur chaque composant). Aujourd'hui bloqué (plan Pro: il faut un siège
Developer en Organization/Enterprise). Tout est déjà prêt dans `figma/`,
`npx figma connect publish` suffira après l'upgrade. Voir `figma/README.md`.

## 10. Garde-fous (à transformer en réflexes)

- Jamais coder un écran sans frame Figma validé.
- Toujours composer en **instances** dans Figma (sinon le design diverge du code).
- Jamais une valeur CSS en dur si un token existe.
- Light **et** Dark vérifiés à chaque fois.
- Le frame Figma validé + la page Audit sont la source de vérité visuelle.

## 11. Démarrage concret

- **Au fil de l'eau, pas en bloc:** chaque fois qu'on touche une feature, on
  crée ou on met à jour son composant Figma d'abord, puis on code. La couverture
  features (aujourd'hui ~25%, voir page Audit) se complète naturellement.
- **Pour une feature neuve:** applique directement le workflow de la section 5.
- **Quand le plan le permet:** activer Code Connect pour fermer la boucle en
  Dev Mode.
