# Premier lancement

Au premier démarrage d'Apollia OS, un onboarding interactif vous guide à travers la configuration et la découverte de l'application. Ce chapitre décrit ce que vous verrez et ce que vous devrez faire.

## Les phases de l'onboarding

L'onboarding se déroule en **6 phases séquentielles** :

1. **Accueil** — présentation d'Apollia OS et sélection du profil
2. **Moteur IA** — configuration d'un modèle local `.gguf` (optionnel)
3. **Setup IA** — détection automatique des modèles LLM et STT présents sur votre machine
4. **Connaissance** — conversation avec un agent pour personnaliser votre expérience
5. **Tour guidé** — visite interactive des fonctionnalités selon votre profil
6. **Graduation** — résumé de votre parcours et accès aux sections principales

Si vous interrompez l'onboarding, une barre de reprise s'affiche en haut de l'application à la prochaine ouverture.

## Choisir son profil

À l'étape d'accueil, vous choisissez entre deux profils :

- **Opérateur** — vous utilisez des agents prêts à l'emploi pour automatiser vos workflows. Le tour guidé vous montrera le tableau de bord, les déclencheurs, les approbations et l'observabilité.
- **Builder** — vous développez des agents Python personnalisés. Le tour guidé couvrira le manifest, la mémoire, le chat agent, les intégrations MCP, les pipelines et l'audit trail.

Vous pouvez également choisir **Les deux** pour accéder aux fonctionnalités complètes.

## Le principe local-first

Tout ce qui se passe pendant l'onboarding reste sur votre machine :

- Les modèles LLM et Whisper détectés sont des fichiers locaux déjà présents dans `~/.apollia/models/` ou `~/Downloads/`
- Aucun téléchargement n'est initié automatiquement
- La conversation d'onboarding (phase Connaissance) s'exécute localement avec le modèle LLM configuré

## Reprendre un onboarding interrompu

Si vous fermez l'application pendant l'onboarding, votre progression est sauvegardée. À la prochaine ouverture, une barre de reprise s'affiche avec les boutons **Reprendre** et **Plus tard**.

Pour réinitialiser l'onboarding manuellement :

```bash
apollia-os reset-onboarding
# ou via l'IPC Tauri depuis l'app desktop
```

> **Référence technique :** [Onboarding-System](https://github.com/nidal-z/apollia-os/wiki/Onboarding-System)
