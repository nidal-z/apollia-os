---
title: L'IA ne répond pas
slug: /operator-help/troubleshooting/the-ai-does-not-answer
sidebar_position: 1
---

# L'IA ne répond pas

> Pour toute personne dont le chat reste figé ou dont le **point d'état à gauche du mot *Apollia*** dans la barre du haut passe à l'ambre ou au rouge : retrouver une IA fonctionnelle en moins de cinq minutes.

## Comprendre le point d'état Apollia

Apollia affiche **un seul indicateur global** de l'état de son moteur et de l'IA, en haut à gauche, à côté du mot *Apollia* :

- 🟢 **vert** - *Moteur connecté · fournisseur LLM disponible* : le moteur tourne et au moins un modèle est prêt à répondre.
- 🟡 **ambre** - *Moteur connecté · aucun fournisseur LLM configuré* : le moteur tourne mais aucun modèle ne peut répondre.
- 🔴 **rouge clignotant** - *Moteur hors ligne - reconnexion en cours* : l'application a perdu son moteur et tente de le rejoindre.

Survolez le point pour lire l'état exact. Ce point est le repère central de cette page.

## Vérifications rapides (par ordre de probabilité)

### 1. Votre connexion réseau est tombée

Un serveur se joint par le réseau. Une coupure Wi-Fi ou VPN coupe le chat de ce serveur, alors qu'un modèle du poste continue de répondre.

**Solution :**
1. Ouvrez un onglet de navigateur pour confirmer que vous êtes en ligne, et que le VPN demandé par votre organisation est actif.
2. Une fois la connexion rétablie, attendez quelques secondes : le point d'état repasse au vert et vous pouvez relancer votre message.

### 2. La clé du serveur est invalide ou expirée

Une clé révoquée, expirée ou recopiée avec un espace en trop fait échouer toutes les requêtes.

**Solution :**
1. Dans la barre latérale, ouvrez **IA**, puis choisissez le serveur sous **Serveurs**.
2. Lisez son état en haut : **Actif**, **Désactivé**, **Injoignable** ou **En erreur**. Survolez l'état pour lire ce qu'il veut dire.
3. Cliquez sur **Tester** pour redemander ses modèles au serveur. Le message dit quoi vérifier : l'adresse, la clé, ou le démarrage du serveur.
4. Si le serveur refuse la clé, cliquez sur **Modifier**, saisissez une clé valide, cliquez de nouveau sur **Tester**, puis sur **Enregistrer**.

### 3. Le modèle n'est plus proposé par le serveur

La personne qui gère un serveur peut renommer ou retirer ses modèles. Un modèle qui n'est plus servi échoue à chaque appel.

**Solution :**
1. Sur l'écran **IA**, choisissez le serveur : les modèles qu'il propose aujourd'hui sont listés sous son état.
2. Si le modèle que vous utilisiez n'est plus dans la liste, ouvrez le menu d'un autre modèle et choisissez **Utiliser par défaut**.
3. Dans une conversation, choisissez ce modèle dans le sélecteur d'agents et de modèles.

### 4. Le serveur est en panne

Un serveur arrêté, ou qu'un incident a fait tomber, affiche l'état **Injoignable**. Si le test échoue alors que l'adresse et la clé sont justes, l'origine est du côté du serveur.

**Solution :**
1. Demandez à la personne qui gère le serveur, ou consultez la page de statut du service concerné.
2. En attendant, ajoutez un autre serveur avec **Ajouter un serveur**, ou téléchargez un modèle avec **Découvrir des modèles**, puis choisissez **Utiliser par défaut** dans le menu du modèle. Un agent dont le modèle propre n'est plus disponible le signale sur sa fiche, sous **Modèle**.

### 5. Le modèle du poste ne répond pas

Un modèle du poste est exécuté par le moteur intégré à Apollia ; Ollama est un service à part, joint comme n'importe quel autre serveur.

**Solution :**
1. Pour **Ollama**, vérifiez que le service Ollama est démarré sur votre poste.
2. Pour un **modèle du poste**, ouvrez **IA** et confirmez que le modèle figure dans **Sur ce poste** et porte le badge **Par défaut**. Une section **Chargés** vide est normale : un modèle se charge à la première question qu'on lui pose.
3. Pour un serveur, cliquez sur **Tester** pour le vérifier de nouveau.

## Si rien ne fonctionne

1. **Quittez complètement Apollia et relancez-la.** Le point d'état repasse au vert si tout va bien.
2. Si le point reste rouge clignotant, le problème est du côté du moteur d'Apollia, pas du modèle. Lisez les journaux dans `~/.apollia/logs/`. **Effacer les logs**, dans le bloc **Zone de danger** de **Réglages → Général**, vide ce dossier pour repartir d'un état propre.
3. Pour être prévenu quand l'IA ne répond plus, activez l'événement **IA indisponible** dans **Réglages → Notifications**.
4. En dernier recours, contactez le support en joignant l'état et le message affichés sur le serveur, avec son nom.

> **Référence technique :** [Référence Apollia](/reference).
