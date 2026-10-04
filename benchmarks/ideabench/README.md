# IdeaBench

Measures how creative a system's *ideas* are, so tinybackrooms can be compared
with plain prompting on the thing it is for: breadth and novelty, not a single
right answer. ([`../creativitybench`](../creativitybench/README.md) covers the
closed-form case, where the conversation did not help.)

## What exists, and why this is new

| Benchmark | What it gives | Gap for us |
| --- | --- | --- |
| [NoveltyBench](https://arxiv.org/abs/2504.05228) | Prompts that allow many answers; *distinct* equivalence classes; patience-discounted cumulative utility | Its curated prompts are mostly mode-collapse probes (names, random picks); few real ideation tasks. Distinctness is not novelty or usefulness |
| AUT / TTCT / DAT | Human-validated divergent-thinking measures | Single-word answers, no constraints |
| LiveIdeaBench, Ideation Arena, LigBench | Research-idea quality | Domain-specific, and expert-human or heavy to run |
| ["Style Wins, Substance Loses"](https://arxiv.org/abs/2608.01666) | Shows LLM judges reward polish and length by 15–30 points | A warning, not a benchmark |

IdeaBench combines them: 32 constrained ideation briefs (26 written for this
benchmark across 10 domains, 3 AUT-style, 3 from NoveltyBench's curated set),
NoveltyBench's distinctness and cumulative utility, and an evaluation protocol
built on the judge-bias findings.

## Protocol

Every arm turns each brief into N ideas (default 5). Then:

1. **Normalise.** A formatter rewrites every idea, from every arm, into a plain
   description of at most 60 words with no praise, headings or lists. No arm can
   win on polish or length.
2. **Rate.** A blind rubric rater scores *novelty* (1 = what a typical assistant
   would answer first) and *usefulness* (would it solve the brief within its
   constraints; at most 2 if it breaks one), each scaled to 0–1. Creativity is
   the product, so an idea has to be both.
3. **Compare.** Each idea is judged head to head against a reference idea from
   the `single` arm, in both orders. Win rate is steadier than absolute scores.
4. **Cluster.** The N ideas are grouped into distinct ideas. *Cumulative
   utility* sums creativity over the first idea of each group, discounted by
   0.8 per group, so repeating yourself earns nothing.

The judge (`google/gemini-3.8-flash`) is a different family from the generator
(`deepseek/deepseek-v4.1-flash`). The engine arms and the evolutionary loop rank
with the generator's family, so neither optimises against the benchmark's own
judge.

## Arms

| Arm | What runs |
| --- | --- |
| `single` | One call asking for N distinct ideas: what most people do |
| `sampled` | N independent calls, one idea each (NoveltyBench's setup) |
| `single-k` | Control: several `single` calls pooled (`--pool`, default 12), then ranked down to N by the evolve reviewer. Matches the engine's compute with no conversation |
| `sampled-k` | Control: `--pool` independent one-idea calls, ranked down to N |
| `vs` | Verbalized sampling ([2510.01171](https://arxiv.org/abs/2510.01171)): one call for 2N ideas with typicality probabilities; keep the N least typical |
| `lens` | N independent calls, each under a different persona lens; no call sees another |
| `stim` | N independent calls, each forced into an analogy to two random unrelated words |
| `brainstorm` | tinybackrooms, one agent, 3 variations × 3 ideas, top N by the `comparative` and `originality` scorers |
| `backrooms` | The same with two agents conversing |
| `evolve` | AlphaEvolve-style: a population of 9, an evaluator ranking them, then three rounds of mutating the three fittest from the reviewer's criticism and recombining the top two |

## Quick check

```sh
cargo build --release -p tinybackrooms-api --features openhuman   # once
python3 benchmarks/ideabench/smoke.py                # 6 briefs, 4 arms, about 15 minutes
python3 benchmarks/ideabench/smoke.py --full --yes   # all 32 briefs, every arm: hours
```

`smoke.py` starts its own API on port 8091 with `deepseek/deepseek-v4.1-flash` as
the generator, runs the arms against the `single` baseline, prints the report and
stops the API. The judge stays Gemini so the generator never grades itself. A
full run needs `--yes` because it spends real money; `--dry-run` shows the plan
and `--arms` overrides the preset.

## Run it

```sh
# the API (see ../creativitybench/README.md for its environment), then:
python3 benchmarks/ideabench/ib.py run --tag v1 --workers 6
python3 benchmarks/ideabench/ib.py report --tag v1
```

`--arms`, `--limit`, `--n`, `--variations`, `--generations` and `--judge-model`
adjust the run. A run resumes where it stopped, and `ib.py judge --tag v1`
re-judges stored ideas with another judge model without regenerating.

## Reading the report

`mean` is the average creativity of the N ideas, `best` the average of each
brief's best idea (what an AlphaEvolve-style search cares about), `winrate` the
head-to-head rate against `single`, `distinct` the average number of distinct
ideas out of N, and `cum.util` NoveltyBench's utility. The paired section gives
each arm's difference from `single` and from `sampled` over the 32 briefs, with a
95% bootstrap interval; a `*` means the interval excludes zero.

## Limits

- One judge model is one opinion. Re-run with `ib.py judge --judge-model …` and
  compare before trusting a small gap.
- The judge is calibrated by its rubric, not by humans. Treat scores as
  comparable between arms in one run, not as absolute creativity.
- 32 briefs gives intervals of a few points. Differences smaller than the
  interval are noise.
- Novelty is judged against the judge's idea of a typical answer, which a model
  that has seen a lot of ideas may misjudge.
