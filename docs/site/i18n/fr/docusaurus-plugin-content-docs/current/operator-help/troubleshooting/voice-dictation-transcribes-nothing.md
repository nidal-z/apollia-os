---
title: La dictée vocale ne transcrit rien
slug: /operator-help/troubleshooting/voice-dictation-transcribes-nothing
sidebar_position: 5
---

# La dictée vocale ne transcrit rien

> Pour toute personne qui appuie sur son raccourci de dictée sans voir apparaître de texte : retrouver une transcription fluide en quelques minutes.

## Vérifications rapides (par ordre de probabilité)

### 1. Le raccourci clavier n'est pas reconnu

Le raccourci de dictée peut entrer en conflit avec un raccourci système (Spotlight, capture d'écran) ou avec une autre application.

**Solution :**
1. Dans la barre latérale, cliquez sur **Réglages**, puis sur la section **Dictée**. Vérifiez que **Activer le moteur STT** est allumé : il arme le raccourci global.
2. Repérez la ligne **Raccourci global** : la combinaison actuelle s'affiche sous forme de touches.
3. Cliquez sur **Modifier le raccourci** : une fenêtre de capture s'ouvre. Maintenez les touches de modification et appuyez sur la nouvelle touche, puis validez avec **Confirmer** ou **Entrée**. Enregistrez la section.
   ![Dialogue de capture de raccourci clavier, en attente d'une combinaison de touches](/img/operator-help/troubleshooting-la-dictee-vocale-ne-transcrit-rien-1.png)
4. Quittez la fenêtre avec **Échap** pour annuler.
5. Testez à nouveau le raccourci : l'enregistrement doit démarrer dès l'appui.

### 2. Le modèle de dictée n'est pas téléchargé

Apollia transcrit sur votre machine avec un modèle de dictée. Sans modèle, la dictée se termine sur **« Aucun modèle de dictée n'est chargé. Choisissez-en un dans les réglages. »**

**Solution :**
1. Ouvrez **Réglages → Dictée** et lisez la ligne **Modèle actif**.
2. Si elle indique **Modèle non chargé**, ouvrez l'écran **IA**, cliquez sur **Découvrir des modèles**, choisissez **Dictée** et téléchargez celui qui porte la mention **Recommandée**. Un fichier de modèle que vous avez déjà s'ajoute avec **Charger un modèle**, dans **Réglages → Dictée**.
3. Revenez dans **Réglages → Dictée**, choisissez le modèle dans la liste **Modèle** et enregistrez : la ligne **Modèle actif** affiche alors son nom.

### 3. Votre microphone système est muet ou mal sélectionné

Si le micro est désactivé au niveau du système d'exploitation, Apollia n'entend rien. Elle le dit au lieu de rester muette : la dictée se termine sur **« Rien d'audible n'a été capté. Vérifiez que le bon microphone est sélectionné et qu'il n'est pas coupé. »**

Un périphérique d'entrée virtuel est une cause fréquente. Des outils comme BlackHole, Soundflower ou un périphérique agrégé apparaissent comme des microphones et peuvent devenir l'entrée par défaut du système : Apollia enregistre alors un flux dans lequel personne ne parle.

**Solution :**
1. Ouvrez **Réglages → Dictée** et utilisez le sélecteur **Microphone** pour nommer explicitement votre microphone, au lieu de vous en remettre à **Périphérique par défaut**.
2. Enregistrez. Le choix s'applique dès la dictée suivante, sans relancer l'application.
3. Cliquez sur **Tester** dans la même page et parlez : les barres doivent suivre votre voix. Des barres plates signifient que le périphérique choisi ne délivre rien.
4. Si rien ne bouge, débranchez et rebranchez le micro (ou ajustez le volume d'entrée).

### 4. Apollia n'a pas l'autorisation d'utiliser le microphone

Au premier usage, le système demande la permission d'accès au micro. Si elle a été refusée, Apollia reste muette.

**Solution :**
1. Dans les réglages **Confidentialité** de votre système, ouvrez la section **Microphone**.
2. Vérifiez qu'**Apollia** figure dans la liste des applications autorisées et que la case est cochée.
3. Si elle ne l'est pas, cochez-la, puis relancez Apollia.

### 5. La langue de transcription ne correspond pas

Le modèle de dictée transcrit selon la langue configurée. Une langue erronée produit du texte incohérent ou rien d'utile.

**Solution :**
1. Dans **Réglages → Dictée**, ouvrez le sélecteur **Langue**.
2. Sélectionnez **Français** (ou la langue effective de votre dictée). Laisser **Détection automatique** confie le choix au modèle, ce qui n'est pas fiable sur un enregistrement court ou bruité.
3. Refaites un essai court de quelques secondes.

### 6. Le micro reste allumé et la dictée ne se termine jamais

<!-- claim:stt-dictation-always-reports-an-outcome -->

Une dictée se termine toujours, qu'elle ait produit du texte ou non. Si le bouton micro reste rouge sans résultat, l'exécution dit pourquoi au lieu de vous laisser attendre : aucun microphone détecté, rien d'audible capté, enregistrement trop court, aucun modèle chargé, ou échec de transcription.

**Solution :**
1. Lisez le message affiché à côté du champ de saisie, ou sous la carte **Tester la dictée** dans **Réglages → Dictée**.
2. Agissez sur ce qu'il nomme : il désigne le périphérique de capture, le modèle, ou l'enregistrement lui-même.
3. Si le bouton reste rouge sans aucun message, c'est un défaut à signaler : l'issue devrait toujours être annoncée.

## Si rien ne fonctionne

1. Allez dans **Transcriptions** *(visible dans la barre latérale avec le mode développeur allumé)* pour voir si des essais récents ont produit du contenu vide ou incohérent : cela aide à localiser le problème.
2. Téléchargez un modèle de dictée plus grand depuis l'écran **IA**, avec **Découvrir des modèles**, si vos dictées sont systématiquement imprécises.
3. Les réglages de dictée s'appliquent dès la dictée suivante, sans relance. Relancer n'est utile que si l'application elle-même ne répond plus.

> **Référence technique :** [Référence Apollia](/reference).
