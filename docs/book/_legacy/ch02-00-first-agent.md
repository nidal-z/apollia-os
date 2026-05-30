# Votre premier agent

Le chapitre précédent vous a montré le contrat minimal : deux méthodes, un fichier, un agent. Mais `hello-agent` ne fait rien d'utile — il répète ce qu'on lui dit.

Dans ce chapitre, vous allez construire quelque chose de réel.

---

## Ce que vous allez construire

Un **assistant fichier** : un agent qui reçoit un chemin de fichier, lit son contenu, le résume via LLM, et sauvegarde le résumé sur le disque.

Commande cible :

```bash
apollia-os run file-assistant "Résume /data/rapport.txt"
```

Résultat attendu :

```
Done in 2.1s (1 step, 3 tool calls)
RESULT
Résumé de /data/rapport.txt :

Ce rapport présente les résultats du T3 2025. Les revenus atteignent 2,4M€
(+18% vs T3 2024). Les charges opérationnelles sont stables à 1,1M€. La
marge brute progresse de 3 points à 54%. Le rapport recommande d'accélérer
le recrutement commercial pour saisir la croissance du segment PME.

Résumé sauvegardé dans : /data/rapport_summary.txt
```

---

## Ce que vous allez apprendre

En construisant cet agent, vous allez :

- Utiliser `ctx.tools` pour lire et écrire des fichiers
- Utiliser `ctx.llm` pour résumer du texte via un LLM
- Gérer les erreurs dans `run()` de manière robuste
- Structurer un agent plus complexe qu'un simple hello-world

Chaque section de ce chapitre suit la progression :

1. **Conception** — que doit faire l'agent ? quels outils ? quel manifest ?
2. **Le manifest** — déclarer les dépendances
3. **Implémenter run** — la logique métier pas à pas
4. **Comprendre les outils** — file_read, file_write, ctx.llm en détail
5. **Tester et exécuter** — le code complet, copier-coller, et l'exécution

> **Prérequis :** un backend LLM configuré dans `apollia.toml`. Si vous n'en avez pas encore, le chapitre 6 explique comment en configurer un. Pour l'instant, vous pouvez lire et comprendre le code — la section "Tester et exécuter" indique comment vérifier votre configuration avant de lancer.
