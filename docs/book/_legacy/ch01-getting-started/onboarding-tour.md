# Tour guidé interactif

Le tour guidé est la phase 5 de l'onboarding. Il vous fait découvrir les fonctionnalités d'Apollia OS par des actions concrètes, dans l'ordre adapté à votre profil.

## Comment fonctionne le tour

Le tour superpose une **carte d'étape** flottante sur l'interface et met en évidence les éléments pertinents via un spotlight. La barre de progression verticale à gauche indique votre avancement.

Exemple — étape Agents (profil Operator) :

```
┌─────────────────────────────┐
│ 2 / 8              [×]      │
│                             │
│ Démarrons un agent          │
│                             │
│ Cliquez sur Démarrer pour   │
│ activer csv-data-worker.    │
│                             │
│ [Précédent]      [Suivant]  │
└─────────────────────────────┘
```

### Navigation

- **Suivant** / **Précédent** — naviguer entre les étapes
- **× (Passer le tour)** — interrompre le tour avec confirmation, progression sauvegardée
- **Touches clavier** — `→` pour avancer, `←` pour reculer, `Échap` pour la confirmation de sortie

### Commandes vocales

Si votre modèle Whisper est configuré, un bouton microphone apparaît pendant le tour. **Maintenez** le bouton pour enregistrer une commande :

| Commande vocale | Action |
|---|---|
| « Suivant » / « Next » | Passe à l'étape suivante |
| « Précédent » / « Back » | Revient à l'étape précédente |
| « Passer » / « Skip » | Ouvre la confirmation de sortie |
| Question quelconque | Envoie la question au Companion Apollia |

## Le Companion Apollia

Pendant tout le tour, le **Companion Apollia** est disponible dans son panneau flottant. Il affiche un message contextuel pour chaque étape et peut répondre à vos questions via le chat.

Pour ouvrir ou fermer le Companion, cliquez sur le bouton en bas à droite de l'écran.

## Reprendre un tour interrompu

Si vous quittez le tour en cours, votre position est persistée. À la prochaine ouverture de l'application, la barre de reprise vous propose de **Reprendre** là où vous en étiez.

> **Référence technique :** [Onboarding-Tour-Steps](https://github.com/nidal-z/apollia-os/wiki/Onboarding-Tour-Steps) — tables complètes des étapes par profil (route, action, message Companion)
