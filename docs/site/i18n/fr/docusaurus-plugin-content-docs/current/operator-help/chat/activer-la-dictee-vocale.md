# Activer la dictée vocale

> Pour tout operator qui veut parler à son IA au lieu de taper : configurer un raccourci clavier qui transcrit votre voix directement dans le champ de chat, en local.

## Prérequis

- Un modèle Whisper est téléchargé via [Télécharger des modèles locaux](../installation/telecharger-des-modeles-locaux.md).
- Le microphone de votre machine fonctionne et Apollia a l'autorisation d'y accéder.
- Un raccourci clavier libre, qui n'entre pas en conflit avec l'OS ou une autre application.

## Étapes

1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Reconnaissance vocale**.
   ![page Paramètres, section Reconnaissance vocale, état du modèle Whisper affiché en haut](/img/operator-help/chat-activer-la-dictee-vocale-1.png)

2. Vérifiez que le modèle Whisper apparaît avec une pastille verte **Chargé**. Sinon, retournez au Hub de modèles pour le télécharger.

3. Sélectionnez la **langue** de dictée dans le sélecteur. Il propose treize langues plus **Détection automatique** ; la liste exacte figure dans [Configuration](/reference/configuration). Nommer la langue améliore nettement la précision, et la détection automatique n'est pas fiable sur un enregistrement court.

4. Sélectionnez votre microphone dans le sélecteur **Périphérique d'entrée**. Laisser le défaut système convient jusqu'au jour où un périphérique virtuel (BlackHole, Soundflower, un périphérique agrégé) devient ce défaut : Apollia enregistre alors du silence. Nommer le périphérique lève l'ambiguïté.

5. Cliquez sur le champ **Raccourci global**. Une fenêtre invite à appuyer sur la combinaison de touches souhaitée (par exemple **Cmd + Shift + Espace**).
   ![fenêtre HotkeyCapture avec message "Appuyez sur la combinaison de touches" et combinaison capturée](/img/operator-help/chat-activer-la-dictee-vocale-2.png)

6. Dans le sélecteur **Mode de déclenchement**, choisissez l'un des deux modes :
   - **Toggle (appui = start/stop)** : un premier appui sur le raccourci démarre l'enregistrement, un second l'arrête.
   - **Push-to-talk (maintien)** : vous maintenez le raccourci pendant que vous parlez, et la transcription se déclenche au relâchement.

   Pour la suite de cette procédure, choisissez **Push-to-talk (maintien)**.

7. Enregistrez. Les réglages de dictée s'appliquent dès la dictée suivante ; il n'y a rien à relancer.

8. Avant de quitter la page, lancez le **Test** et dites quelques mots. Les barres doivent suivre votre voix, et le texte reconnu s'affiche en dessous. Des barres plates signifient que le périphérique choisi ne délivre rien.

9. Ouvrez un chat depuis la sidebar.

10. Maintenez votre raccourci enfoncé. Un **overlay sombre plein écran** s'affiche avec un visualiseur audio à barres. Parlez naturellement.
    A l'ecran : l'overlay d'enregistrement plein écran, avec le visualiseur audio à barres et le texte {hotkey} pour arrêter · Esc pour annuler.

11. Relâchez le raccourci. La transcription est injectée dans le champ de saisie via le presse-papiers.

    > **Note :** la transcription est insérée par simulation de collage (`Ctrl+V` / `Cmd+V`). Le champ de saisie doit être actif pour recevoir le texte.

12. Relisez, corrigez si besoin, puis appuyez sur **Entrée** pour envoyer.

## Vérification

Une phrase parlée de quelques secondes apparaît transcrite dans le champ de saisie, une seule fois, sans qu'aucune donnée n'ait quitté votre machine.

## Ce qui se passe quand rien n'a été entendu

<!-- claim:stt-refuses-silent-audio -->

Le silence n'est pas transcrit. Quand chaque instant d'un enregistrement se situe sous le seuil de silence, Apollia annonce que rien d'audible n'a été capté au lieu d'envoyer l'audio au modèle. Cela compte, parce qu'un modèle de reconnaissance vocale à qui l'on donne du silence ne rend pas un résultat vide : il rend des phrases plausibles qu'on ne lui a jamais dites, et elles parvenaient auparavant à l'operator avec l'apparence de vraies transcriptions.

## Si ça ne marche pas

- **Aucune transcription :** nommez votre microphone dans le sélecteur **Périphérique d'entrée** au lieu de vous en remettre au défaut système, puis consultez [La dictée vocale ne transcrit rien](../troubleshooting/la-dictee-vocale-ne-transcrit-rien.md).
- **Raccourci ignoré :** une autre application capte peut-être la même combinaison ; choisissez-en une moins courante.
- **Transcription approximative :** vérifiez que la langue sélectionnée correspond bien à celle parlée, et envisagez un modèle Whisper plus gros.
- **Le texte n'apparaît pas dans le champ :** assurez-vous que le champ de saisie du chat est actif (cliquez dessus) avant d'utiliser le raccourci.

> **Référence technique :** [Référence Apollia](/reference) - moteurs supportés, tailles de modèles Whisper, formats audio, optimisations de latence.
