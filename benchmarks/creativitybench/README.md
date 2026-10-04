# CreativityBench harness

Runs [CreativityBench](https://creativitybench.github.io/) against
tinybackrooms and against plain prompting, so the backrooms can be compared with
the cheapest alternative on a published creativity benchmark.

## What the benchmark measures

CreativityBench ([paper](https://arxiv.org/abs/2605.02910),
[code](https://github.com/CreativityBench/CreativityBench)) is **affordance-based
tool repurposing**. A task gives a scene (several entities, each described part
by part, plus loose items) and a goal that the entities' usual purposes do not
cover. The model must name the **entity**, the **part** and **how to use** it.
It is closed-form: every task has a gold entity-part and a gold solution.

Scoring, from the benchmark's own `judge.py`:

- entity and part match the gold (exact, or word-F1 fuzzy);
- otherwise the pair's relation to gold (`similar-N`, `not similar`,
  `hallucination`) from annotations shipped in each task;
- an LLM judge scores `how_to_use` on condition coverage, attribute grounding,
  correctness against the gold solution, and physical feasibility (0–2).

The 3,285-task release has 1,884 `difficult`, 753 `low` and 648 `medium` tasks.

## Why it fits, and what it does not test

The engine's job is to explore several candidate answers and select one. This
benchmark has a verifiable answer, so it shows whether conversation plus
selection finds a better answer than one direct call. It is a
creativity-as-problem-solving test. It does **not** measure open-ended novelty
(AUT, DAT, NoveltyBench style), which the engine's own diversity metrics cover.

## Arms

| Arm | What runs | Selection |
| --- | --- | --- |
| `static` | The benchmark's static prompt, one call, temperature 0 | none |
| `static-k` | The same prompt, K calls at the run temperature | none (first sample) |
| `brainstorm` | tinybackrooms, one agent, K variations | engine's judge scorer |
| `backrooms` | tinybackrooms, two agents, K variations × G turns | engine's judge scorer |

The engine never sees the gold answer. The tinybackrooms arms get the scene as
`input.brief`, a three-field `output.schema` (`gold_entity`, `gold_part`,
`how_to_use`), and rank their K answers with the engine's own judge scorer.

## Metrics

| Column | Meaning |
| --- | --- |
| `top1 exact` | rank-1 answer's entity and part equal the gold, case-sensitive |
| `top1 gold` | the benchmark's own match (exact or fuzzy word-F1) |
| `any@K gold` | any of the K answers is gold (an upper bound; needs the gold to pick) |
| `mean gold` | share of the K answers that are gold |
| `top1 ok` | rank-1 is gold or `similar-*` to gold |
| `quality` | mean of correctness, grounding, feasibility, scaled to 0–1 |
| `distinct` | distinct (entity, part) pairs among the K answers |
| `tok/task`, `s/task` | cost |

`top1 gold` is more generous than `top1 exact`: the benchmark snaps a made-up
part name onto the nearest real one by word overlap. Read both.

## Run it

```sh
# once: clone the benchmark, download the task release (~400 MB)
python3 benchmarks/creativitybench/cb.py setup

# a seeded subset, 20 tasks per difficulty
python3 benchmarks/creativitybench/cb.py sample --out /tmp/cb60.json --per-difficulty 20

# the API, backed by OpenHuman agents on OpenRouter
cargo build --release -p tinybackrooms-api --features openhuman
TINYBACKROOMS_BACKEND=openhuman \
TINYBACKROOMS_OPENHUMAN_API_KEY="$OPENROUTER_API_KEY" \
TINYBACKROOMS_OPENHUMAN_BASE_URL=https://openrouter.ai/api/v1 \
TINYBACKROOMS_OPENHUMAN_MODEL=deepseek/deepseek-v4.1-flash \
TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS=240 \
  target/release/tinybackrooms-api &

# the comparison
python3 benchmarks/creativitybench/cb.py run --tasks /tmp/cb60.json --tag demo \
  --variations 3 --generations 4 --workers 8
python3 benchmarks/creativitybench/cb.py report --tag demo
```

Results append to `runs/<tag>/results.jsonl` and a run resumes where it stopped.
Needs `OPENROUTER_API_KEY` (generation of the static arms, and the judge). The
generator is `--model`; the judge is `--judge-model` (default a different model
family from the generator, to limit self-preference).

## Deviations from the benchmark's protocol

- **Transport.** Their `models.py` is replaced by OpenRouter; their prompts and
  judge are imported unchanged from the checkout, never copied here.
- **Part-attribute hints.** Their judge reads `1_entity_lookup.json` for wrong
  answers. That file comes from their task-creation stage and is not in the
  release, so the hints are empty.
- **Judge model.** They used `gpt-5.2`. Scores are comparable between arms in
  one run, not with their leaderboard.
- **One sample per task, temperature 0 for `static`.** They also run 0.3 and
  0.7 and judge only the zero-temperature sample.
- **Subset.** Runs use a stratified sample, so differences of a few points are
  inside the noise.
