# ADR-010 — Pivot du SaaS Python vers le Runtime Rust open-source

**Date :** 2026-03
**Statut :** Accepté (décision fondatrice)
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Apollia existait initialement comme un SaaS full-stack (FastAPI + SvelteKit) pour la gestion d'agents IA pour les PME. Après 8 mois de développement, l'analyse révèle : marché SaaS multi-agent très encombré (AutoGen, CrewAI, LangGraph, n8n cloud), cycle de vente long avec les PME, ressources insuffisantes pour concurrencer les plateformes bien financées sur le segment SaaS, et difficulté à différencier un SaaS généraliste.

En revanche, le noyau technique d'Apollia (ORIA, sandbox, mémoire souveraine) représente une valeur architecturale réelle et unique — particulièrement le principe "local-first, zéro cloud". Ce noyau peut être extrait et distribué comme infrastructure open-source.

## Décision

Nous arrêtons le développement SaaS full-stack. Nous extrayons le noyau technique dans un runtime Rust open-source : `apollia-os`. Ce runtime est distribué comme binaire unique sous licence MIT. Il héberge des agents Python (LangGraph, CrewAI, custom) en local, sans cloud. Le modèle économique devient services/conseil autour du runtime plutôt que SaaS.

## Alternatives considérées

### Option A — Continuer le SaaS Python (rejetée)
**Pour :** Infrastructure existante, retour sur investissement potentiel.
**Contre :** Marché encombré. Cycle de vente long PME. Ressources insuffisantes pour rivaliser. Différenciation faible vs. les plateformes bien financées.

### Option B — SaaS avec runtime Python (rejetée)
**Pour :** Cohérence de stack, vitesse de développement.
**Contre :** Viole les principes #1 (données en cloud) et #2 (dépendances système Python). Reproductions des problèmes du SaaS précédent.

### Option C — Open-source le SaaS complet (rejetée)
**Pour :** Community-led development, adoption large.
**Contre :** Trop complexe à opérer pour la communauté (stack complète FastAPI + SvelteKit + agents). La valeur est dans le runtime, pas dans le frontend.

### Option retenue — Runtime Rust open-source
**Pour :** Différenciation technique réelle (local-first, binaire unique). Adopté sans friction par les développeurs d'agents existants. Modèle viable solo en 8-10h/semaine.
**Compromis acceptés :** Abandon de 8 mois de code SaaS. La valeur architecturale (ORIA, mémoire, sandbox) est conservée. Nouveau modèle économique à construire.

## Conséquences

**Positives :**
- Focus technique sur la valeur réelle : runtime souverain, local-first.
- Binaire unique adoptable par tout développeur Python en 5 minutes.
- Contribution possible à l'écosystème open-source des agents IA.
- Architecture plus simple et maintenable en ressources contraintes.

**Négatives / Compromis :**
- 8 mois de code SaaS abandonnés (FastAPI, SvelteKit, base de données cloud).
- Modèle économique moins direct que le SaaS (pas d'abonnement récurrent immédiat).
- Nécessite de construire une communauté open-source from scratch.

**Neutres / À surveiller :**
- Adoption par les développeurs d'agents dans les premiers mois.
- Identification des use cases qui génèrent de la valeur économique (support, consulting, enterprise).

## Principes architecturaux impactés

- Tous les 8 principes sont directement issus de cette décision fondatrice.
- Principe #1 — Local-first : raison d'être du pivot.
- Principe #2 — Zéro dépendance externe : différenciateur vs. SaaS cloud.

## Liens

- Story associée : STORY-001 (Init workspace Cargo — première concrétisation du pivot)
- Documentation associée : `docs/Vision-Pivot-et-Renouveau.md`
- ADR précédent sur le même sujet : aucun (décision fondatrice)
