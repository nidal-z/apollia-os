# Adversarial verification of the rewritten sweep analysis

An attempt to prove the wave C verdicts wrong. Not a review of the work.

The operation that rewrote `sweep.py`'s analysis stated plainly that it had not
run adversarial verification against its own corrected analysis. Four pieces of
code were new and unchecked, and every "detectable effect" and "no detectable
effect" verdict in wave C rests on them:

| code | file | what it decides |
|---|---|---|
| `detection_threshold_pct` | `sweep.py:1258` | the bar a delta must clear |
| `curve_analysis`, `curve_points`, `cluster_lengths` | `sweep.py:1498-1620` | the per-length prefill curve comparison |
| the baseline-instability guard in `analyse` | `sweep.py:1355-1372` | when a metric's verdicts are withheld |
| `ceiling_check`, `render_ubatch_differential`, `ubatch_response` | `sweep.py:2153-2415` | the discriminator behind README 1.12 finding 4 |

Wave C means the verdicts rendered by `sweep.py --report` against
`results/sweep-qwen-full-grid-20260729T150416Z.json` and
`results/sweep-ministral-confirmation-20260729T162235Z.json`, which is what
README 1.12 findings 4, 5, 6 and 8 state, plus the discriminator table in
finding 4. That is **205 verdicts**: 83 metric-table rows and 70 curve rows on
the hybrid grid, 28 and 24 on the dense one.

Nothing here was fixed. `sweep.py`, `harness.py`, `roofline.py`,
`sweep_selftest.py` and `README.md` are byte-identical to how this pass found
them. Two files were added: `sweep_nulltest.py`, the instrument for attacks 1
and 2, and this report.

---

## Summary

| # | attack | verdict |
|---|---|---|
| 1 | false-positive rate of the threshold statistic | **refuted** for the per-comparison rate, **confirmed** for unequal dispersion and for multiplicity |
| 2 | what the statistic misses | **confirmed**, and it splits wave C in two |
| 3 | the baseline-instability guard on its own fixture | **confirmed**: it fires on a correlate of the defect, and does not reach the curve table at all |
| 4 | does `--ceiling-check` compute the claimed discriminator | **confirmed** on direction-blindness and on absence-versus-withholding, **refuted** on the arithmetic and the 3.1 to 3.9 range |
| 5 | a verdict without a comparison | **confirmed** on four paths, **refuted** on three |
| 6 | do the nine self-test cases assert behaviour | **confirmed**: the two newest guards have no coverage, and one case asserts itself |

No wave C verdict on the clean grids is shown to be **wrong**. Several are shown
to be **unable to carry the weight** README 1.12 puts on them, and one claim
about the code in finding 9 is shown to be **false as stated**. The named list is
at the end.

---

## Attack 1: false-positive rate against synthetic null data

`sweep_nulltest.py` builds sweep containers whose baseline runs and level run are
drawn from one distribution, so no effect exists by construction, and drives
`sweep.analyse` over them. Anything the report calls an effect is a false
positive. The bare-cv rule the threshold replaced is scored on the same
replicates, so the argument that motivated the replacement is checked rather than
repeated.

The instrument is calibrated before it is believed: with the t critical value
forced to 1e9 the effect rate must be 0, and forced to 0 it must be 100. It
reaches both.

10,000 replicates per configuration:

```
  configuration                                 effect   1-sigma  withheld   cv seen  thr seen
  n=5  between 0.0012, qwen prefill cold         5.08 %    42.07 %     0.00 %    0.0011     0.34 %
  n=5  between 0.0139, qwen decode cold          5.14 %    43.32 %     0.00 %    0.0126     3.84 %
  n=5  between 0.0155, dense decode cold         5.42 %    42.75 %     0.00 %    0.0142     4.32 %
  n=5  between 0.0368, dense warm ttft           4.86 %    41.91 %     0.00 %    0.0337    10.25 %
  n=3  between 0.0139                            0.00 %    49.20 %   100.00 %    0.0115     5.71 %
  n=8  between 0.0139                            4.83 %    38.54 %     0.00 %    0.0133     3.33 %
  n=12 between 0.0139                            5.51 %    37.95 %     0.00 %    0.0134     3.08 %
  n=20 between 0.0139                            5.11 %    35.00 %     0.00 %    0.0137     2.93 %
  n=5  between 0.0139, lognormal                 5.19 %    41.54 %     0.00 %    0.0127     3.88 %
  n=5  between 0.0139, within 0.007              4.99 %    42.84 %     0.00 %    0.0131     3.98 %
  n=5  between 0.0139, within 0.05               5.12 %    42.63 %     1.95 %    0.0276     8.41 %
  n=5  between 0.0139, level 2x dispersion      22.33 %    65.63 %     0.00 %    0.0126     3.82 %
  n=5  between 0.0139, level 3x dispersion      38.25 %    76.89 %     0.00 %    0.0127     3.87 %
  n=5  between 0.0012, level 2x dispersion      21.27 %    64.49 %     0.00 %    0.0011     0.33 %
```

The `thr seen` column reproduces the real grids' thresholds: 0.34 against 0.4,
3.84 against 4.2, 4.32 against 4.7, 10.25 against 11.2. The synthetic
comparisons are being made at the bar the real verdicts were made at.

### Refuted: the per-comparison rate is right

**4.83 to 5.51 percent across eight homoscedastic configurations, against a
nominal 5.** At 10,000 replicates the standard error on a 5 percent rate is 0.22
points, so every one of those sits within statistical noise of nominal. The rate
does not drift with `n` (5, 8, 12 and 20 all land on nominal), does not move
under a lognormal sample shape, and does not move when within-run noise is added.

This refutes a specific prediction made before the measurement. The statistic is
a t-interval half-width derived for a **mean**, and it is applied around the
baseline **median**; the median of five observations carries about 0.287 sigma
squared against 0.2 for the mean, so `sqrt(1 + 1/n)` understates the true factor
and the rate should have come in above nominal. It does not. The cv is estimated
from the same five runs that supply the reference, and the two errors evidently
offset. The prediction was wrong and the code is right.

The `n = 3` row is the sub-five-runs guard working: 100 percent withheld, 0
percent effect. No verdict is rendered at all below the I8 floor.

### Substantiated: the argument for the replacement

The bare-cv one-sigma rule the threshold replaced fires on **35.0 to 49.2
percent** of null comparisons on the same replicates. README 1.12 finding 7 says
"roughly a third". The real figure is about 42 percent at n = 5. The claim is
substantiated and, if anything, understated.

### Confirmed: unequal dispersion

The homoscedastic case is not the case a sweep is in. A level changes the
configuration, and a configuration can change how reproducible the machine is.
The statistic assumes the level run carries the baseline's dispersion, because
one run cannot estimate its own.

| level's true dispersion | measured false-positive rate |
|---|---|
| equal to baseline | 5.14 % |
| 2x baseline | 22.33 % |
| 3x baseline | 38.25 % |

This is not hypothetical on this data. The hybrid grid's per-slot decode at 8
slots carries a within-run cv of **0.194** against a baseline of 0.0095, and the
dense grid's 4-slot run reaches **0.233**. Those are dispersion ratios of 20 and
30, far past the 3x row. The instability guard catches those two specific rows
because their within-run cv crosses 0.10, which is the saving grace. It is a
proxy, not a measurement of the quantity that matters, and attack 3 shows what
happens when the proxy and the damage come apart.

### Confirmed: 205 comparisons, no multiplicity control

The per-comparison rate is correctly 5 percent. Wave C renders 205 comparisons.
Nothing anywhere corrects for that. If a majority of those are true nulls, the
expected number of false effects in the two grids is around five to six, and
there is no way to say which. README 1.12 finding 7 discusses the threshold at
length and does not mention multiplicity.

Two rows in the grids look like what that predicts, and neither is cited by any
finding:

- hybrid grid, `n_ctx = 65536`, warm decode, **+2.5 percent, "better, beyond the
  2.4 percent threshold"**, while `131072` gives -0.4 percent and `8192` gives
  -0.6 percent. A larger context window making warm decode faster at one level
  and not at the level twice its size is not a shape a mechanism produces.
- hybrid grid curve, `n_batch = 4096` at 4079 tokens, **-0.1 percent, "worse,
  beyond the 0.1 percent threshold"**, while the other five lengths of the same
  level all say no detectable effect.

Naming them is not proving them false. It is saying where the expected handful
of false effects most likely sits.

---

## Attack 2: what the statistic misses

Same instrument, a known multiplicative effect injected. The cell is the
fraction detected with the correct sign, 10,000 replicates each:

```
  configuration                                                 0.5 %    1.0 %    2.0 %    4.0 %    8.0 %   16.0 %
  qwen prefill cold,  between 0.0012, grid threshold 0.4 %      80.2%    99.9%   100.0%   100.0%   100.0%   100.0%
  qwen decode cold,   between 0.0139, grid threshold 4.2 %       4.7%     7.6%    17.9%    51.8%    96.2%   100.0%
  dense decode cold,  between 0.0155, grid threshold 4.7 %       4.3%     7.3%    15.8%    44.2%    92.4%   100.0%
  dense warm ttft,    between 0.0368, grid threshold 11.2 %      3.8%     4.0%     6.4%    12.3%    34.4%    81.9%
  qwen aggregate,     between 0.0042, grid threshold 1.3 %      14.1%    39.6%    88.8%   100.0%   100.0%   100.0%

    qwen prefill cold    50 % power below 0.5 %,   80 % power below 0.5 %
    qwen decode cold     50 % power at 3.89 %,     80 % power at 6.54 %
    dense decode cold    50 % power at 4.48 %,     80 % power at 6.97 %
    dense warm ttft      50 % power at 10.62 %,    80 % power at 15.68 %
    qwen aggregate       50 % power at 1.21 %,     80 % power at 1.82 %
```

**Confirmed, and the shape of it matters more than the fact.** Wave C's claims of
absence do not all rest on the same footing. They split cleanly by metric:

**Well powered, and their absences hold.** Prefill on the hybrid grid resolves
below half a percent with 80 percent power; on the dense grid the threshold is
0.2 percent, tighter still. Aggregate throughput reaches 80 percent power at 1.8
percent. Every claim of absence resting on prefill, cold TTFT or aggregate
throughput is a real claim: finding 6's "a context window is close to free until
it is filled" is carried by prefill at -0.1 percent and cold TTFT at +0.1 percent
against thresholds of 0.4 and 0.6, and it survives this attack intact.

**Underpowered, and their absences say almost nothing.** Decode carries a
between-run cv an order of magnitude larger than prefill on both models, so its
threshold lands at 4.2 and 4.7 percent and its 80 percent power point at 6.5 and
7.0 percent. Warm TTFT on the dense grid is worse: a 4 percent effect is caught
12 percent of the time and an 8 percent effect 34 percent of the time.

Every observed decode delta against `-ub` sits between -1.6 and +1.4 percent, a
region where the test fires between 8 and 18 percent of the time. "No detectable
effect" is being read in README 1.12 finding 4 as "no effect, and it could not
have been otherwise". The measurement supports the first half and not the second.

### Inconclusive: a trend the analysis never tests

The four hybrid-grid decode deltas against `-ub` are strictly monotone in the
flag: 128 gives +1.2 percent, 256 gives -0.8, 1024 gives -1.2, 2048 gives -1.6.
Under a null of exchangeable deltas the probability of that specific ordering is
1/24, about 4 percent. No individual level clears the 4.2 percent bar and the
report has no trend test, so a pattern across four levels is discarded four
times over.

This is stated as **inconclusive** and not as a finding. The ordering was noticed
after the data was seen, which is exactly the circumstance in which a 1-in-24
coincidence should be expected to turn up somewhere in a grid of this size. What
is not inconclusive is the structural point: the analysis compares each level to
the baseline in isolation and has no way to use the fact that levels of a factor
are ordered.

---

## Attack 3: the baseline-instability guard on the dataset that motivated it

`results/contaminated/sweep-qwen-full-grid-20260729T134337Z.json` is a real
dataset with a known defect, which makes it the right regression fixture. README
1.12 finding 9 says `sweep.py` "now checks both sides and withholds every verdict
for a metric whose baseline contains an unstable run", and that re-rendering the
fixture refuses 7 of its 8 metrics, "which is the check working on the data that
motivated it".

### (a) It fires, and for the stated reason. Confirmed as far as it goes.

`python3 sweep.py --report results/contaminated/...` refuses seven metrics, each
naming the offending run and its cv: run 0 at 0.42 on cold decode, runs 0 and 1
at 0.39 and 0.29 on warm decode, run 1 at 0.38 and 0.46 on the two concurrency
metrics. The eighth metric, `toolcall score`, is refused too but for an unrelated
reason (`n` below 5). The README's "7 of its 8" is literally correct.

### (b) Confirmed: it keys on a correlate of the defect, not on the defect

The damage finding 9 describes is in the **between-run** cv: 0.2517 and 0.2964,
producing thresholds of 76.5 and 90.1 percent, which swallowed a real effect. The
guard triggers on the **within-run** cv. On this fixture the two happen to travel
together. They do not have to.

`mutate.py` shrinks each contaminated baseline record's samples toward that
record's own median. Every run median is unchanged, so the between-run cv and
every threshold derived from it are unchanged. Only the within-run cv moves.
Re-rendered:

```
  aggregate_decode_tps_wall     threshold 76.5 %      (identical to the unmutated fixture)
      2      66.46   +26.6 %   no detectable effect
      4      71.86   +36.9 %   no detectable effect
      8      76.44   +45.6 %   no detectable effect
  decode_tps per slot           threshold 90.1 %      (identical)
      2      57.98   -25.8 %   no detectable effect
      4      34.80   -55.5 %   no detectable effect
```

That is exactly the failure finding 9 describes, reproduced on the fixture that
motivated the fix, with the fix in place: a 45.6 percent aggregate throughput
gain and a 55.5 percent per-slot collapse both reported as no detectable effect,
against thresholds the guard no longer objects to.

The mutation is not exotic. Contamination that is steady inside a five-sample run
and differs between runs, which is what a competing job present for the first two
runs and absent afterwards looks like, produces precisely this signature: high
between-run dispersion, ordinary within-run dispersion. That is arguably the more
likely contamination shape than one that jitters within a run.

### (c) Refuted: it does not over-fire

Injecting one inflated baseline run into the clean hybrid grid, holding its
median fixed, withholds five metrics and names run 9 with the right cv on each.
Cold decode and cold TTFT keep their verdicts, because run 9's within-run cv on
those two metrics stayed below 0.10. The guard is per metric, at the right
granularity, and it names rather than drops.

### (d) Confirmed, and this is the largest finding of the pass: the guard does not reach the curve

`curve_points` (`sweep.py:1498`) reads each curve point's `median` and discards
its `cv`. `curve_analysis` filters on `record_disqualifiers` and nothing else: no
instability check on either side, no propagation of the metric table's
`provisional` list, no `row_verdict` at all. It calls `verdict()` directly.

On the contaminated fixture, the curve baselines are contaminated too:

```
  run 0   519:0.0112  1031:0.0119  2095:0.2921  4079:0.0007  8190:0.0015  16372:0.1871
  run 1   519:0.0129  1031:0.3636  2095:0.0287  4079:0.0007  8190:0.0008  16372:0.0007
  (clean grid, same points, same runs: nothing above 0.0295)
```

Three points sit at 0.19, 0.29 and 0.36, two to three times the bar the guard
enforces everywhere else. The same command that refuses 7 of 8 metrics goes on to
print **70 per-length curve verdicts** from those baselines, including verdicts
of the form "worse, beyond the 0.8 percent threshold".

Two of them are inverted by the contamination:

| length, level | contaminated | clean |
|---|---|---|
| 16372 tok, `kv_cache_type = q4_0` | -7.8 %, no detectable effect | -7.8 %, worse, beyond 0.2 % |
| 16372 tok, `kv_cache_type = q8_0` | -9.4 %, no detectable effect | -9.5 %, worse, beyond 0.2 % |

Same delta, opposite verdict, contamination the only difference. That is the
finding-9 failure mode, alive and unguarded, on the fixture finding 9 presents as
proof the failure mode is closed.

**The claim in finding 9 that the code "withholds every verdict for a metric
whose baseline contains an unstable run" is false as stated.** It withholds every
verdict in the metric table. The curve table is a second, parallel comparison
path that re-implements the verdict logic and carries almost none of the guards.

---

## Attack 4: does `--ceiling-check` compute the discriminator finding 4 claims?

### (a) Refuted as a defect: the arithmetic is what it says

`roofline.py:549` sets `decode_ceiling = bandwidth / bytes_per_token`, so
`decode_efficiency_pct = 100 * decode / decode_ceiling` and
`achieved_bps / peak_bps = decode * bytes_per_token / bandwidth` are the same
expression. The ACHIEVED EFFECTIVE BANDWIDTH table is `decode_efficiency_pct`
restated, and README 1.12 finding 4 says so in the sentence that introduces it.
Correct, and honestly labelled. It follows that the table is a restatement of
finding 2 rather than independent evidence for it, which is worth keeping in mind
when reading the section, but it is not a defect.

### (b) Confirmed: the discriminator is direction-blind

`render_ubatch_differential` computes `magnitudes[-1] / magnitudes[0]` over the
two models' absolute prefill deltas and never reads `is_moe`. Finding 4's
inference requires the larger response to belong to the mixture of experts model.
The code asserts nothing about that.

Swapping the two grids' `model_label`, so that the same numbers describe the
opposite architectures, produces:

```
  prefill response to -ub, relative to the engine default
    model                        ub 128      ub 256     ub 1024     ub 2048
    qwen3.6-35b-a3b                  -      -5.7 %      +4.5 %      +5.4 %
    ministral-3-8b             -43.8 %     -21.5 %     +13.8 %     +21.1 %

  The larger response exceeds the smaller by 3.1 to 3.9 times across
  the levels both models ran.
```

That is a world in which the dense full-attention model carries the large
batch-amortisable cost and the mixture of experts model does not, which refutes
the gather reading outright. The concluding sentence is identical, word for word,
including the range. The table above it does carry the model labels, so a careful
reader can recover the direction; the sentence the finding quotes as its result
cannot.

### (c) Refuted: the 3.1 to 3.9 range is right

Recomputed by hand from the two grids: 21.5/5.7 = 3.77, 13.8/4.5 = 3.07,
21.1/5.4 = 3.91. Printed range 3.1 to 3.9. `ub 128` is correctly excluded from
`shared` because the dense model did not run it. README's "3.1 to 3.9 times at
every level both models ran" is exact.

### (d) Confirmed: a withheld decode reads as a confirmed absence

`ubatch_response` sets `detected` from whether a row's verdict starts with
"better" or "worse". A row that was **withheld** contributes nothing to
`response["decode"]` at all, and `render_ubatch_differential` prints "no
detectable effect at any level" whenever `moved` is empty. Absence of a
measurement and absence of an effect are the same output.

Making the four `n_ubatch` level records unstable on decode, holding their
medians fixed, the main report says:

```
    decode_tps cold
      1024   49.55  -35.8 %   unstable, within-run cv 0.41 above the 0.10 of 1.4.8
      128    50.74  -34.2 %   unstable, within-run cv 0.41 above the 0.10 of 1.4.8
      2048   49.34  -36.0 %   unstable, within-run cv 0.41 above the 0.10 of 1.4.8
      256    49.74  -35.5 %   unstable, within-run cv 0.41 above the 0.10 of 1.4.8
```

and `--ceiling-check` on the same file says:

```
  decode response to -ub, same runs
    qwen3.6-35b-a3b        no detectable effect at any level
```

Four withheld measurements averaging -35 percent, rendered as a confirmed
absence. On the real grids the decode rows are genuinely comparable and do say
"no detectable effect", so the wave C output is not produced by this path. The
sentence simply has no capacity to distinguish the two cases, and finding 4's
structural argument quotes it.

---

## Attack 5: a verdict without a comparison

Seven paths constructed and run through the real CLI.

**Confirmed, a verdict where nothing was compared:**

- **A factor whose only level is the baseline level.** With five identical
  baseline runs the threshold is 0.0 percent, and a level nominally identical to
  the baseline receives `+0.7 %  better, beyond the 0.0 % threshold`. `verdict`
  appends a parenthetical, `comparable` is still True, and `ubatch_response`
  would still count it as detected.
- **Zero threshold, zero delta.** A level 0.001 percent from the baseline is
  reported as `+0.0 %   better, beyond the 0.0 % threshold`. The delta column and
  the verdict column contradict each other on the same line. Reachable today only
  because `toolcall score` in both grids has a between-run cv of exactly 0.0000
  and a threshold of exactly 0.0 percent, and is saved by I8 rather than by the
  threshold: its aggregate has n = 1. A future scored probe reporting five
  observations walks straight into this.
- **A curve verdict from a single baseline run.** `curve_analysis` gates on
  `length["n_runs"] >= 5`, but `n_runs` counts pooled rate values, not runs. One
  baseline run measuring five points inside one cluster clears the gate: a
  constructed case yields `1100 tok  baseline 1000.0  level 1060.0  +6.0 %
  better, beyond the 4.8 % threshold`, where the 4.8 percent "threshold" is the
  dispersion across five different prompt lengths within one run. The metric
  table refuses this outright (`len(baseline_values) < 5`). The curve does not.
  A plan-time guard at `sweep.py:322` rejects lengths spaced closer than the
  match ratio, which closes this for planned sweeps but not for the rendered
  dataset.

**Confirmed, a measurement that vanishes:**

- **A metric with levels and no baseline.** A concurrency level that ran and
  produced an aggregate, against a campaign with no concurrency baseline, is
  dropped by `analyse`'s `continue`. Nothing is printed, the record id appears
  nowhere, and the exit code is 0. The only announcement is the total-absence
  case ("No metric had both a baseline and a level"). The report's own standard,
  that a run that happened should be readable, is not met here.

**Refuted:**

- A level whose records were excluded is surfaced in the header count and returns
  exit 1, with the exclusion reason printed.
- A level record with samples but no stats block is named under RECORDS THAT
  ENTERED NO COMPARISON, exit 1.
- A model whose `-ub` rows are all provisional does not produce a one-sided
  discriminator. `render_ubatch_differential` falls back to "Pass the sweep
  datasets for both models to compute it", which reads as an instruction and
  cannot be mistaken for a result.

---

## Attack 6: do the nine self-test cases assert behaviour?

Ten guards removed one at a time from a scratch copy of `sweep.py` outside the
repository, running the unmodified `sweep_selftest.py` against each. The
unmodified copy prints 9/9 before any mutation.

```
  guard removed                              cases that noticed   verdict
  G1  baseline-instability guard              none                NO CASE NOTICED
  G2  detection threshold, the t factor       none                NO CASE NOTICED
  G3  level-instability guard                 A3                  covered
  G4  warm-sample guard                       A4                  covered
  G5  invariant disqualification              A6                  covered
  G6  I8 provisional guard                    A5                  covered
  G7  curve point consumption                 none                NO CASE NOTICED
  G8  curve absent-from-baseline report       A8b                 covered
  G9  declared-gate announcement              B3                  covered
  G10 mixed cache_state report                A7                  covered
```

**Confirmed. The two newest pieces of code have no regression coverage.**

- **G1.** Replacing the baseline-instability block with `unstable_baseline = []`
  passes all nine. The guard README 1.12 finding 9 presents as the fix for the
  contaminated grid is not covered by any case.
- **G2.** Replacing `t_crit * cv * math.sqrt(1.0 + 1.0/n)` with `cv`, which is
  the exact one-sigma rule the rewrite was undertaken to remove, passes all nine.

**Confirmed. A8a asserts itself.** Its stated purpose is that "one level point
must not satisfy two baseline lengths", and it counts rows to prove no double
count. Removing the `remaining.pop(chosen)` that provides that guarantee does not
make it fail. The reason is in its own fixture: the baseline lengths are 1024 and
1200, `log(1200/1024) = 0.1586` is below the 0.20 match ratio, so
`cluster_lengths` merges them into a single length before the pop is reached:

```
  clusters from the A8a baseline: [[1024, 1024, 1024, 1024, 1024, 1200, 1200, 1200, 1200, 1200]]
```

There is only ever one length for the level's single point to match, so the
double count the case is named for cannot occur in it whatever the code does.
The fixture also happens to demonstrate the `n_runs` defect from attack 5: ten
pooled values from two different nominal lengths, counted as ten runs.

**Refuted for A2.** Its headline claim, that the pooled record-level prefill
metric is gone, is genuinely covered: adding `prefill_tps curve` back to
`METRICS` drops the suite to 8/9 with A2 the failure.

**Confirmed by inspection.** No case reaches `--ceiling-check`; the suite only
ever invokes `--report`.

---

## Wave C verdicts this pass invalidates, downgrades, or leaves standing

### Invalidated

No verdict on either clean grid is shown to be wrong. Both clean grids' baselines
are stable at every metric (the guard does not fire on either), the per-comparison
threshold is correctly calibrated, and the discriminator range reproduces exactly.

One claim about the code is invalidated:

- **README 1.12 finding 9**, the sentence "`sweep.py` now checks both sides and
  withholds every verdict for a metric whose baseline contains an unstable run".
  False as stated. It withholds every verdict in the metric table and none in the
  per-length curve table, and on the contaminated fixture the curve table emits
  70 unguarded verdicts, two of them inverted by the contamination. The
  neighbouring sentence, "7 of its 8 metrics are refused, which is the check
  working on the data that motivated it", is literally true and leaves an
  impression the same command's output contradicts eighteen lines further down.

### Downgraded from "no effect" to "underpowered"

The verdicts stand as printed. What they cannot bear is the weight README 1.12
puts on them.

| finding | factor and model | levels | why |
|---|---|---|---|
| 4 | `n_ubatch`, cold decode, qwen3.6-35b-a3b | 128, 256, 1024, 2048 | observed deltas -1.6 to +1.2 %, detected 8 to 18 % of the time at that size; 80 % power needs 6.5 % |
| 4 | `n_ubatch`, cold decode, ministral-3-8b | 256, 1024, 2048 | observed +0.2 to +1.3 %, 80 % power needs 7.0 % |
| 4 | `n_ubatch`, warm decode, both models | all levels | same thresholds, same power |
| 8 | `n_ubatch`, warm TTFT, ministral-3-8b | 256, 1024, 2048 | observed -4.6 to +4.5 % against an 11.2 % threshold; a 4 % effect is caught 12 % of the time |
| 6 | `n_batch`, cold and warm decode, qwen3.6-35b-a3b | 512, 1024, 4096 | observed -0.7 to -1.2 % against a 4.2 % threshold |

The consequence for finding 4 is specific. Its structural conclusion reads
"Decode showed no detectable effect from `-ub` on either model, at any level, and
it could not have". The second clause is an argument from first principles about
batch-of-one decoding and is untouched by this pass. The first clause is a
measurement, and as a measurement it is close to uninformative at the effect
sizes in question. The conclusion may well be right; the decode measurement is
not what makes it right.

Finding 8's dense warm TTFT row is downgraded on the same grounds, and its
conclusion survives on other evidence: the recomputed-token column is 1 at every
level, which is a direct observation and not a threshold comparison.

### Flagged as the likeliest false positives in the grids

Neither is cited by any finding. With 205 comparisons at a correctly calibrated 5
percent and no multiplicity control, roughly five to six false effects are
expected, and these two are the ones whose shape does not fit a mechanism:

- hybrid grid, `n_ctx = 65536`, warm decode, +2.5 % against a 2.4 % threshold,
  non-monotone against both neighbouring levels.
- hybrid grid curve, `n_batch = 4096` at 4079 tokens, -0.1 % against a 0.1 %
  threshold, alone among that level's six lengths.

### Standing after every attack

- **Finding 6's `-c` result**, that raising the context window from 32768 to
  131072 costs 0.5 percent of prefill and 0.7 percent of cold first token and
  that 8192 is indistinguishable from baseline. Carried entirely by prefill and
  cold TTFT, which reach 80 percent power below 0.5 percent. This is the
  best-supported claim of absence in wave C.
- **Finding 6's `-np` result.** Aggregate throughput reaches 80 percent power at
  1.8 percent, and the observed gains are 27, 41 and 48 percent. The per-slot
  collapse is reported alongside, and the 8-slot per-slot median is correctly
  withheld as unstable.
- **Finding 5**, the KV quantisation cost. Deltas of -6.8 to -11.0 percent on
  decode against 4.2 and 4.7 percent thresholds sit at 92 to 96 percent power,
  and the prefill deltas are far past their 0.2 to 0.4 percent bars.
- **Finding 4's prefill differential**, 3.1 to 3.9 times, reproduced by hand.
  What does not survive is the tool's ability to state which architecture the
  larger response belongs to.
- **Findings 1, 2 and 3** were not in scope: they rest on the prefix-collapse
  intervention series, the roofline arithmetic and the agentic turn
  decomposition rather than on the rewritten threshold code.

---

## How to reproduce

```sh
cd scripts/model-eval

# attacks 1 and 2, about 40 seconds
python3 sweep_nulltest.py 10000

# attack 3a, the fixture as it stands
python3 sweep.py --report results/contaminated/sweep-qwen-full-grid-20260729T134337Z.json

# attack 4b and 4c
python3 sweep.py --ceiling-check results/baseline-*.json results/sweep-*.json
```

Attacks 3b, 3c, 4b, 4d, 5 and 6 use mutated datasets and a scratch copy of
`sweep.py` built outside the repository, so they left nothing behind. The
mutation is described in each section precisely enough to rebuild: shrink or
inflate each record's samples around that record's own median, which moves the
within-run cv and holds the run median, and therefore the between-run cv and
every threshold, fixed.

## What this pass did not do

Nothing was fixed. Every defect above is left standing, by design: finding and
fixing in one motion is how an adversary becomes an author and stops being an
adversary.

Three attacks came back refuted and are worth stating as plainly as the
confirmations. The threshold statistic holds its nominal rate to within
measurement noise across eight configurations, three sample sizes and two
distribution shapes. The argument that motivated it was understated rather than
overstated. The discriminator's headline number reproduces exactly by hand. A
prediction made in this pass before measuring, that the median-versus-mean
mismatch would inflate the false-positive rate, was wrong.
