# Faire tourner un modèle volumineux (shardé)

> **Référence technique :** [Briques-LLM-Backend](https://github.com/nidal-z/apollia-os/wiki/Briques-LLM-Backend#modeles-gguf-multi-fichiers-shards) — pattern de nommage officiel, table des erreurs de validation, schémas JSON complets.

Les modèles open-weights les plus capables — Llama-70B, Mixtral-8x22B, DeepSeek-V3 — pèsent entre 40 GB et 400 GB une fois quantisés. Ils sont toujours distribués en plusieurs fichiers GGUF : les limites filesystem (FAT32 ≤ 4 GB, partages réseau), les téléchargements reprenables, et la convention de l'écosystème llama.cpp imposent ce format.

Apollia charge ces modèles shardés nativement. Aucun ré-assemblage manuel — techniquement impossible de toute façon, puisque les shards GGUF ont des en-têtes distincts et qu'une simple concaténation produit un fichier invalide.

---

## Exemple pratique — Llama-70B-Instruct Q5_K_M

### 1. Télécharger les shards

Le pattern standard de nommage est `<prefix>-NNNNN-of-MMMMM.gguf` (5 chiffres zero-padded, `NNNNN` = index, `MMMMM` = total). HuggingFace, Ollama et `llama-quantize --split` produisent directement ce format.

```bash
cd ~/.apollia/models/
# Adapter l'URL à la variante HuggingFace choisie.
curl -L -O https://huggingface.co/.../Llama-70B-Instruct-Q5_K_M-00001-of-00003.gguf
curl -L -O https://huggingface.co/.../Llama-70B-Instruct-Q5_K_M-00002-of-00003.gguf
curl -L -O https://huggingface.co/.../Llama-70B-Instruct-Q5_K_M-00003-of-00003.gguf
```

### 2. Vérifier avec `apollia model list`

```
$ apollia model list
  Models directory: /Users/nidal/.apollia/models

  NAME                                             LAYOUT                       SIZE
  Llama-70B-Instruct-Q5_K_M                        3 shards                     49152.0 MB
  Qwen3-8B-Q5_K_M.gguf                             mono                          5800.0 MB
```

Si la colonne `LAYOUT` affiche `2/3 shards (INCOMPLETE)`, un shard a échoué au téléchargement — retenter le shard manquant et rejouer `apollia model list`.

### 3. Configurer le backend

Pointer `model_path` sur le **premier shard** suffit. Apollia valide la série au démarrage ; llama.cpp charge ensuite automatiquement les shards suivants.

```toml
[[llm.backends]]
type         = "embedded"
name         = "llama-70b"
model_path   = "~/.apollia/models/Llama-70B-Instruct-Q5_K_M-00001-of-00003.gguf"
quantization = "Q5_K_M"
device       = "metal"   # ou "cuda" / "cpu"
```

### 4. Démarrer

```bash
$ apollia start
[INFO] chargement modèle GGUF split — llama.cpp auto-charge les shards suivants shards=3 prefix="Llama-70B-Instruct-Q5_K_M"
[INFO] modèle local prêt backend=llama-70b
```

---

## Diagnostic en cas d'erreur

Apollia valide la série **avant** d'initialiser llama.cpp (Principe #4 — Fail fast), ce qui garantit des messages d'erreur rapides et orientés correction :

- **`ModelShardMissing`** — un shard manque sur le disque. Le premier chemin absent détecté est reporté dans le champ `expected` de l'erreur. Retélécharger ce shard suffit à relancer.
- **`ShardIndexNotFirst`** — la config pointe sur un shard autre que `00001`. llama.cpp n'accepte que le premier shard comme point d'entrée — remplacer par `-00001-of-…` et relancer.
- **`ModelNotFound`** — le chemin fourni est incorrect. Vérifier avec `apollia model list` que le fichier apparaît bien sous le nom attendu.

---

## Naming scheme non standard

Certaines distributions communautaires produisent des splits au nommage arbitraire (ex : `model_a.gguf`, `model_b.gguf`) qui ne suivent pas le pattern `-NNNNN-of-NNNNN`. Dans ce cas, utiliser `model_paths` (liste ordonnée, mutuellement exclusif avec `model_path`) :

```toml
[[llm.backends]]
type         = "embedded"
name         = "custom-split"
model_paths  = [
  "~/.apollia/models/mymodel_a.gguf",
  "~/.apollia/models/mymodel_b.gguf",
  "~/.apollia/models/mymodel_c.gguf",
]
quantization = "Q4_K_M"
device       = "cpu"
```

L'ordre de la liste est respecté tel quel.

---

## Créer ses propres splits

Pour découper un GGUF monolithique en shards (utile pour un transport filesystem contraint) :

```bash
llama-quantize --split --split-max-size 30G in.gguf out.gguf Q5_K_M
```

`llama-quantize` est fourni par le projet llama.cpp upstream et produit directement le pattern standard `-NNNNN-of-NNNNN` qu'Apollia détecte automatiquement.
