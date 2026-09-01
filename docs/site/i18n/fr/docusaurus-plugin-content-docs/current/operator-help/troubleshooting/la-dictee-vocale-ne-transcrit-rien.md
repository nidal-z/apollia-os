---
title: La dictée vocale ne transcrit rien
slug: /operator-help/troubleshooting/voice-dictation-transcribes-nothing
sidebar_position: 5
---

# La dictée vocale ne transcrit rien

> Pour tout operator qui appuie sur son raccourci de dictée sans voir apparaître de texte : retrouver une transcription fluide en quelques minutes.

## Vérifications rapides (par ordre de probabilité)

### 1. Le raccourci clavier n'est pas reconnu

Le raccourci de dictée peut entrer en conflit avec un raccourci système (Spotlight, capture d'écran) ou avec une autre application.

**Solution :**
1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Reconnaissance vocale**.
2. Repérez la ligne **Raccourci global** : la combinaison actuelle s'affiche sous forme de touches stylisées.
3. **Cliquez sur la combinaison** : un dialog plein écran de capture s'ouvre. Appuyez sur la nouvelle combinaison souhaitée - elle est capturée en temps réel et enregistrée à la validation.
   ![Dialogue de capture de raccourci clavier, en attente d'une combinaison de touches](/img/operator-help/troubleshooting-la-dictee-vocale-ne-transcrit-rien-1.png)
4. Quittez le dialog avec **Échap** pour annuler.
5. Testez à nouveau le raccourci : **un overlay d'enregistrement avec visualizer audio** doit s'afficher dès l'appui.

### 2. Le modèle de transcription n'est pas téléchargé

Apollia transcrit en local avec un modèle Whisper. Sans modèle chargé, l'appui sur le raccourci ne produit rien.

**Solution :**
1. Ouvrez **Paramètres → Reconnaissance vocale** et regardez l'état du moteur en haut de la page.
2. Si l'état affiche *« Modèle non chargé »* ou équivalent, ouvrez **Paramètres → Hub de modèles** et téléchargez au minimum le modèle **Whisper Small** (suffisant pour le français).
3. Revenez sur la page **Reconnaissance vocale** : l'état doit afficher **Modèle chargé**.

### 3. Votre microphone système est muet ou mal sélectionné

Si le micro est désactivé au niveau du système d'exploitation, Apollia n'entend rien. Elle le dit au lieu de rester muette : la dictée se termine sur **« Rien d'audible n'a été capté »**.

Un périphérique d'entrée virtuel est une cause fréquente. Des outils comme BlackHole, Soundflower ou un périphérique agrégé apparaissent comme des microphones et peuvent devenir l'entrée par défaut du système : Apollia enregistre alors un flux dans lequel personne ne parle.

**Solution :**
1. Ouvrez **Paramètres → Reconnaissance vocale** et utilisez le sélecteur **Périphérique d'entrée** pour nommer explicitement votre microphone, au lieu de vous en remettre au défaut système.
2. Enregistrez. Le choix s'applique dès la dictée suivante, sans relancer l'application.
3. Lancez le **Test** de la même page et parlez : les barres doivent suivre votre voix. Des barres plates signifient que le périphérique choisi ne délivre rien.
4. Si rien ne bouge, débranchez et rebranchez le micro (ou ajustez le volume d'entrée).

### 4. Apollia n'a pas l'autorisation d'utiliser le microphone

Au premier usage, le système demande la permission d'accès au micro. Si elle a été refusée, Apollia reste muette.

**Solution :**
1. Dans les réglages **Confidentialité** de votre système, ouvrez la section **Microphone**.
2. Vérifiez qu'**Apollia** figure dans la liste des applications autorisées et que la case est cochée.
3. Si elle ne l'est pas, cochez-la, puis relancez Apollia.

### 5. La langue de transcription ne correspond pas

Le modèle Whisper transcrit selon la langue configurée. Une langue erronée produit du texte incohérent ou rien d'utile.

**Solution :**
1. Dans **Paramètres → Reconnaissance vocale**, ouvrez le sélecteur **Langue**.
2. Sélectionnez **Français** (ou la langue effective de votre dictée). Laisser **Détection automatique** confie le choix au modèle, ce qui n'est pas fiable sur un enregistrement court ou bruité.
3. Refaites un essai court de quelques secondes.

### 6. Le micro reste allumé et la dictée ne se termine jamais

<!-- claim:stt-dictation-always-reports-an-outcome -->

Une dictée se termine toujours, qu'elle ait produit du texte ou non. Si le bouton micro reste rouge sans résultat, l'exécution dit pourquoi au lieu de vous laisser attendre : aucun microphone détecté, rien d'audible capté, enregistrement trop court, aucun modèle chargé, ou échec de transcription.

**Solution :**
1. Lisez le message affiché à côté du champ de saisie, ou sous la carte **Test** dans **Paramètres → Reconnaissance vocale**.
2. Agissez sur ce qu'il nomme : il désigne le périphérique de capture, le modèle, ou l'enregistrement lui-même.
3. Si le bouton reste rouge sans aucun message, c'est un défaut à signaler : l'issue devrait toujours être annoncée.

## Si rien ne fonctionne

1. Allez dans **Transcriptions** *(visible dans la sidebar en mode Builder)* pour voir si des essais récents ont produit du contenu vide ou incohérent : cela aide à localiser le problème.
2. Téléchargez un modèle Whisper plus précis (**Medium** ou **Large**) depuis **Paramètres → Hub de modèles** si vos dictées sont systématiquement floues.
3. Les réglages de dictée s'appliquent dès la dictée suivante, sans relance. Relancer n'est utile que si l'application elle-même ne répond plus.

> **Référence technique :** [Référence Apollia](/reference) - comprendre comment Apollia capte, traite et stocke vos dictées en local.
