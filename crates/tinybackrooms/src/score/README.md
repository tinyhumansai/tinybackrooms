# score

Scores, ranks and summarizes the pooled candidates.

## Design

Each candidate gets a value in `[0, 1]` from every configured scorer; the
ranking key is their weighted mean. Ranking uses a stable sort, so ties keep
variation order and a fixed backend gives a fixed ranking.

- `mod.rs`: `rank`, `item_text`, the `elaboration` scorer and the set-level
  `Diversity`.
- `math.rs`: cosine distance, nearest-neighbour distance, DAT, greedy
  distinct-class clustering, and the `Vendi` score (Jacobi eigenvalues of the
  similarity matrix).
- `judge.rs`: the multi-rater judge.
- `comparative.rs`: one reviewer ranks the whole pool side by side.

## Scorers

| Name | Meaning |
| --- | --- |
| `originality` | Cosine distance to the nearest other candidate or remembered idea |
| `elaboration` | Distinct words in the item text / 40, capped at 1 |
| `judge` | Mean over raters of `novelty × usefulness`, each scaled 1–7 to `[0, 1]` |
| `comparative` | Rank in one reviewer's side-by-side ordering, held to `scoring.criteria`; best 1, worst 0 |

The judge also reports `judge_disagreement` (unweighted) and a majority Boden
class. A failed or out-of-range rating is dropped; an item with no usable rating
scores 0 and the run gets a warning.

## Constraints

- Scorers are not rescaled against each other.
- `Vendi` considers at most 128 items (cubic cost).
- The default `HashingEmbedder` measures surface overlap, not meaning. Treat
  `originality` and `vendi` as proxies until a learned `Embedder` is plugged in.
- Full rationale and references: `docs/specs/scoring.md`.
