# Baseline de non-regression ESRS

Reference figee pour detecter une regression de qualite de la classification ESRS de l'agent
Yumni. Comparaison de SCORES, pas rejeu de trace runtime : on gele un score attendu et on
recompare un run frais, on ne reexecute pas une trace enregistree.

## Fichiers

- `baseline-predictions.json` : predictions de reference, gelees depuis un run reel deterministe
  du modele configure de l'agent (`Ministral-3-8B-Instruct-2512-Q5_K_M.gguf`, temperature 0.0,
  seed 42). Source : `scripts/model-eval/results/ministral-3-8b.json` (bloc `esrs`).
- `baseline-scores.json` : rapport de scores attendu, sortie de `eval/score.py --json` sur les
  predictions de reference. Micro F1 = 0.857 (P=0.9, R=0.818, tp=9, fp=1, fn=2 sur 10 samples).
- `check_baseline.py` : recalcule le score d'un run frais et le compare au plancher
  (F1 baseline moins tolerance). Sort non-zero sur regression.
- `criteria.sample.json` : liste fermee des codes ESRS (referentiel du prompt et du scorer).

## Pourquoi ce choix de modele

L'agent Yumni est configure sur Ministral-3-8B (`agents/yumni-classification/apollia.toml`). La
baseline gele donc le run reel de ce modele exact, pas un modele hypothetique. Les deux scoreurs
independants (le probe `scripts/model-eval/esrs_probe.py` et `eval/score.py`) donnent le meme
resultat (F1=0.857), ce qui valide la reference.

## Usage

Produire un run frais (director sur chaque sample, ou probe modele), au format attendu par
`score.py` (`samples: [{id, predicted: [codes]}]`), puis :

```sh
# score brut d'un run frais
python eval/score.py --pred eval/predictions.json

# garde de non-regression contre la reference figee
python eval/fixtures/check_baseline.py --pred eval/predictions.json
# tolerance par defaut 0.05 ; ajuster avec --tolerance
```

Une regression = micro-F1 du run frais sous `F1_baseline - tolerance`. Le jeu d'eval est un jeu
PoC de 10 samples (labels indicatifs, pas verite terrain) : la baseline detecte une derive
grossiere, pas une variation fine. Rafraichir la baseline (regeler depuis un nouveau run reel) si
le modele configure change.
