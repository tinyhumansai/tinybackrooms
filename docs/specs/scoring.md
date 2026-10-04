# Scoring and diversity

## Status and owner

Implemented. Owner: tinyhumans.

## Problem

"Creative" has no ground truth. The literature offers proxies that disagree, so
the engine ships several, makes them configurable, and says what each measures.

## Background: what the benchmarks measure

| Measure | What it is | Used here as |
| --- | --- | --- |
| Alternative Uses Task / TTCT (Guilford, Torrance) | Fluency, flexibility, originality, elaboration of divergent ideas | `fluency`, `flexibility` (set), `elaboration`, `originality` (item) |
| Divergent Association Task (Olson 2021) | Mean pairwise semantic distance of 10 unrelated words, ×100 | `dat` (set) |
| NoveltyBench (arXiv 2504.05228) | Number of distinct equivalence classes among k samples | `distinct_classes` |
| Vendi score (arXiv 2210.02410) | exp(entropy of the similarity-matrix eigenvalues): effective number of distinct items | `vendi` |
| Artificial Hivemind / Infinity-Chat (arXiv 2510.22954) | Homogeneity within and across models | novelty penalty against memory |
| Consensual Assessment Technique (Amabile) | Several independent raters, holistic rating | `judge` with N raters |
| Boden's taxonomy | Combinational / exploratory / transformational creativity | `boden_class` label |
| EQ-Bench creative writing | Rubric-driven LLM judge | informs the judge prompt |

Caveats from that literature that shaped the design: purely semantic distance
tracks human originality ratings poorly (r≈0.19 in one comparison) while a
fine-tuned LLM scorer does much better (r≈0.80, OCSAI; different data, so a
rough comparison), and automatic metrics disagree with each other. Hence
several scorers, a quality gate, and a recommendation to calibrate.

## Proposed behavior

Every per-item scorer returns a value in `[0, 1]`. The ranking key is the
weighted mean over the configured scorers.

- **`originality`**: cosine distance to the nearest *other* candidate or
  remembered idea (novelty search). `0.5` when there is nothing to compare to.
- **`elaboration`**: distinct lowercase words in the item's text, divided by 40,
  capped at 1.
- **`judge`**: `judge_raters` independent calls each rate novelty and
  usefulness 1–7 and label a Boden class. An item scores the mean over raters of
  `((novelty−1)/6) × ((usefulness−1)/6)`; the product is the quality gate, so a
  merely strange idea cannot rank well. `judge_disagreement` (population
  standard deviation across raters) is reported, unweighted. Out-of-range
  ratings and failed calls are dropped with a warning; an item with no usable
  rating scores 0.
- **`comparative`**: one reviewer call sees every candidate in the pool side by
  side, in a shuffle fixed by `config.seed`, and ranks them against
  `scoring.criteria` (default: how well each serves the brief). It writes a
  short assessment first, then the ranking. A candidate's score is
  `1 − place/(n−1)`: best 1, worst 0; a candidate the reviewer omits is placed
  after those it ranked. A pool of fewer than two costs no call and scores 1; a
  failed or unusable call scores everything 0 with a warning. Use it when the
  task has a right answer to find. The reviewer is `models.judge`.

  *Why it exists.* On CreativityBench (a closed-form creative problem-solving
  benchmark with gold answers) the per-item `judge` ranked the correct
  candidate first about half the time it was present; this scorer ranked it
  first every time on the 12-task pilot. Rating a candidate alone lets a
  plausible flaw through; seeing the alternatives exposes it.

The returned set also gets `Diversity`: `vendi`, `distinct_classes` (greedy
leader clustering at `class_threshold`), `fluency` (count), `flexibility`
(`distinct_classes / fluency`) and `dat`. `vendi` considers at most the first 128
ranked items because the eigendecomposition is cubic.

## Invariants and constraints

1. Scorers are not rescaled against each other; weights apply to raw values.
2. The default embedder hashes character trigrams, so "semantic" distance is
   surface overlap. A learned embedder goes behind the `Embedder` port.
3. A judge or comparison failure never fails a run.
4. Ranking uses a stable sort, so ties keep variation order.

## Acceptance criteria

- Vendi is 1 for identical items and `n` for orthogonal ones; DAT is 100 for
  orthogonal pairs; the judge ranks 4×4 above 7×1; a remembered idea scores 0
  originality; repeated runs in one namespace penalize repeats.

## Open questions

- Calibrate default weights on a small human-labelled set before trusting them.
- A learned embedder would make `originality` and `vendi` meaningfully semantic.
- TypeSafe AI's Jev could back the `judge` scorer as a fast classifier; see
  [ADR 0004](../adr/0004-jev-as-optional-judge.md).
