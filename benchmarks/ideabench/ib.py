#!/usr/bin/env python3
"""IdeaBench: measure how creative a system's ideas are, AlphaEvolve-style.

Standard library only. Every arm turns the same brief into N ideas; the ideas
are then judged blind by a model from a different family.

Evaluation protocol (each step answers a known weakness of LLM idea judging):

1. Normalise. A formatter rewrites every idea from every arm into the same
   short, neutral description, so no arm wins on polish or length.
2. Rate. A rubric rater scores novelty (against what a typical assistant would
   answer) and usefulness (does it solve the brief within its constraints);
   an idea's creativity is their product, so it must be both.
3. Compare. Each idea is also judged head to head against a reference idea from
   the `single` arm, in both orders, which is steadier than absolute scores.
4. Cluster. The arm's N ideas are grouped into distinct ideas (NoveltyBench's
   "distinct" idea), and a patience-discounted cumulative utility counts only
   the first idea of each group.

The judge never shares a family with the generator, and the evolutionary loop's
own fitness function is the generator's family, so the loop cannot overfit to
the benchmark's judge.

Subcommands: run, report.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "creativitybench"))
import cb  # noqa: E402  (shared OpenRouter and HTTP transport)

RUNS = HERE / "runs"
LOCK = threading.Lock()
ARMS = ("single", "sampled", "brainstorm", "backrooms", "evolve")
PATIENCE = 0.8

FIELDS = ("title", "concept", "mechanism", "first_step")
IDEA_SCHEMA = {
    "type": "object",
    "required": list(FIELDS),
    "properties": {
        "title": {"type": "string", "description": "A short name for the idea."},
        "concept": {"type": "string", "description": "What the idea is, in one or two sentences."},
        "mechanism": {"type": "string", "description": "How it works, and why that would work."},
        "first_step": {"type": "string", "description": "The cheapest concrete first step to try it."},
    },
}
CRITERIA = (
    "Prefer ideas that are novel (not what a typical assistant would answer first) and genuinely "
    "useful (they would really solve the brief within its constraints, and a first step is clear). "
    "A merely strange idea ranks below a workable one, and a merely workable stock idea ranks below a "
    "workable surprising one. Penalise candidates that repeat each other: the best set is varied."
)


# --- helpers -----------------------------------------------------------------


def brief_text(prompt):
    text = prompt["brief"]
    if prompt["must"]:
        text += "\nConstraints:\n" + "\n".join(f"- {m}" for m in prompt["must"])
    return text


def jparse(text):
    """The first JSON value found in `text`, or None."""
    text = (text or "").strip()
    for candidate in (text, *re.findall(r"\{.*\}|\[.*\]", text, re.DOTALL)):
        try:
            return json.loads(candidate)
        except (ValueError, TypeError):
            continue
    return None


def as_idea(raw):
    if not isinstance(raw, dict):
        return None
    idea = {k: str(raw.get(k, "")).strip() for k in FIELDS}
    return idea if idea["title"] and idea["concept"] else None


def idea_text(idea):
    return f"{idea['title']}. {idea['concept']} {idea['mechanism']} First step: {idea['first_step']}"


def tokens(usage):
    return usage.get("prompt_tokens", 0) + usage.get("completion_tokens", 0)


class Meter:
    """Counts tokens and calls across threads for one job."""

    def __init__(self):
        self.tokens, self.calls, self._lock = 0, 0, threading.Lock()

    def chat(self, model, messages, temperature):
        text, usage = cb.chat(model, messages, temperature)
        with self._lock:
            self.tokens += tokens(usage)
            self.calls += 1
        return text


SYSTEM = "You are a creative ideation partner. Reply with JSON only."
ASK_ONE = (
    "{brief}\n\nGive ONE idea. Reply with a JSON object with the fields title, concept, mechanism "
    "and first_step."
)
ASK_MANY = (
    "{brief}\n\nGive {n} distinct ideas that differ from each other in kind, not just in detail. "
    'Reply with JSON: {{"ideas": [{{"title": ..., "concept": ..., "mechanism": ..., "first_step": ...}}]}}.'
)


# --- arms --------------------------------------------------------------------


def arm_single(prompt, args, meter):
    text = meter.chat(
        args.model,
        [{"role": "system", "content": SYSTEM},
         {"role": "user", "content": ASK_MANY.format(brief=brief_text(prompt), n=args.n)}],
        args.temperature,
    )
    parsed = jparse(text)
    raw = parsed.get("ideas", []) if isinstance(parsed, dict) else (parsed or [])
    return [i for i in map(as_idea, raw) if i][: args.n]


def one_idea(prompt, args, meter, extra=""):
    text = meter.chat(
        args.model,
        [{"role": "system", "content": SYSTEM},
         {"role": "user", "content": ASK_ONE.format(brief=brief_text(prompt)) + extra}],
        args.temperature,
    )
    return as_idea(jparse(text))


def arm_sampled(prompt, args, meter):
    with ThreadPoolExecutor(max_workers=args.n) as pool:
        got = list(pool.map(lambda _: one_idea(prompt, args, meter), range(args.n)))
    return [i for i in got if i]


def arm_engine(prompt, args, meter, mode):
    request = {
        "input": {"brief": prompt["brief"], "constraints": {"must": prompt["must"], "must_not": []}},
        "output": {"item_name": "idea", "item_guidance": "One concrete idea.", "schema": IDEA_SCHEMA},
        "config": {
            "variations": args.variations, "generations": args.generations,
            "items_per_variation": args.items_per_variation, "mode": mode,
            "template": args.template, "temperature": args.temperature, "seed": args.seed,
            "scoring": {
                "scorers": [{"kind": "comparative", "weight": 1.0}, {"kind": "originality", "weight": 0.5}],
                "select_top_k": args.n, "criteria": CRITERIA,
            },
            "budget": {"wall_clock_secs": args.wall_clock},
        },
    }
    reply = cb.http_json(f"{args.api}/v1/generate", request, {"Content-Type": "application/json"},
                         timeout=args.wall_clock + 180, retries=1)
    usage = reply.get("usage", {})
    with meter._lock:
        meter.tokens += usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
        meter.calls += usage.get("calls", 0)
    return [i for i in (as_idea(x["value"]) for x in reply["items"]) if i][: args.n]


RANK = """You are a strict ideation reviewer.

BRIEF
{brief}

CANDIDATE IDEAS
{ideas}

{criteria}

Reply with JSON only: {{"ranking": [<idea numbers, best first>], "flaws": {{"<number>": "<its most serious weakness, or 'none'>"}}}}"""

MUTATE = """BRIEF
{brief}

CURRENT IDEA
{idea}

A reviewer's main criticism: {flaw}
Other ideas already in the pool (do not resemble them): {others}

Write a better idea. Fix the criticism, make it more surprising while still really solving the brief
within its constraints, and keep it different from the pool. It may change direction entirely.
Reply with a JSON object with the fields title, concept, mechanism and first_step."""

RECOMBINE = """BRIEF
{brief}

TWO IDEAS
[A] {a}
[B] {b}

Combine what is strongest in each into one new idea that is better than both: more surprising and
more workable. Reply with a JSON object with the fields title, concept, mechanism and first_step."""


def rank_pool(prompt, pool, args, meter, seed):
    order = list(range(len(pool)))
    random.Random(seed).shuffle(order)
    shown = "\n".join(f"[{n + 1}] {idea_text(pool[k])}" for n, k in enumerate(order))
    text = meter.chat(
        args.model,
        [{"role": "user", "content": RANK.format(brief=brief_text(prompt), ideas=shown, criteria=CRITERIA)}],
        0.0,
    )
    parsed = jparse(text) or {}
    ranked, seen = [], set()
    for n in parsed.get("ranking", []):
        try:
            k = order[int(n) - 1]
        except (ValueError, IndexError, TypeError):
            continue
        if k not in seen:
            seen.add(k)
            ranked.append(k)
    ranked += [k for k in range(len(pool)) if k not in seen]
    flaws = {}
    for n, flaw in (parsed.get("flaws") or {}).items():
        try:
            flaws[order[int(n) - 1]] = str(flaw)
        except (ValueError, IndexError, TypeError):
            pass
    return ranked, flaws


def arm_evolve(prompt, args, meter):
    """Population, an automated evaluator, and mutation of the fittest."""
    size = args.items_per_variation * args.variations
    with ThreadPoolExecutor(max_workers=size) as pool:
        pop = [i for i in pool.map(lambda _: one_idea(prompt, args, meter), range(size)) if i]
        for round_ in range(args.generations):
            if len(pop) < 3:
                break
            ranked, flaws = rank_pool(prompt, pop, args, meter, f"{prompt['id']}{round_}{args.seed}")
            parents = ranked[:3]
            jobs = []
            for k in parents:
                others = "; ".join(pop[j]["title"] for j in range(len(pop)) if j != k)
                jobs.append(MUTATE.format(brief=brief_text(prompt), idea=idea_text(pop[k]),
                                          flaw=flaws.get(k, "none"), others=others))
            jobs.append(RECOMBINE.format(brief=brief_text(prompt), a=idea_text(pop[parents[0]]),
                                         b=idea_text(pop[parents[1]])))

            def ask(job):
                text = meter.chat(args.model, [{"role": "system", "content": SYSTEM},
                                               {"role": "user", "content": job}], args.temperature)
                return as_idea(jparse(text))

            children = [c for c in pool.map(ask, jobs) if c]
            pop = [pop[k] for k in ranked[: max(3, size - len(children))]] + children
        ranked, _ = rank_pool(prompt, pop, args, meter, f"{prompt['id']}final{args.seed}")
    return [pop[k] for k in ranked[: args.n]]


def generate(prompt, arm, args, meter):
    if arm == "single":
        return arm_single(prompt, args, meter)
    if arm == "sampled":
        return arm_sampled(prompt, args, meter)
    if arm == "evolve":
        return arm_evolve(prompt, args, meter)
    return arm_engine(prompt, args, meter, arm)


# --- normalise and judge -----------------------------------------------------

POLISH = """Rewrite the idea below as a plain, neutral description of at most 60 words: what it is, how it
works, and what is new about it. Do not add or drop any substance, do not praise it, no marketing
language, no headings or lists. Reply with the description only.

IDEA
{idea}"""


def polish(idea, args, meter):
    text = meter.chat(args.model, [{"role": "user", "content": POLISH.format(idea=idea_text(idea))}], 0.0)
    text = (text or "").strip()
    return text if text else " ".join(idea_text(idea).split()[:60])


RATE = """You are a strict rater of ideas. Be sceptical: most ideas deserve middling scores.

BRIEF
{brief}

IDEA
{idea}

novelty (1-7): 1 = what a typical assistant would answer first; 4 = a thoughtful, non-obvious idea;
7 = genuinely surprising and unlikely to have been proposed before.
usefulness (1-7): would this really solve the brief within its constraints? 1 = no; 4 = partly or
with big unknowns; 7 = clearly yes. If it breaks any constraint, usefulness is at most 2.

Reply with JSON only: {{"novelty": <1-7>, "usefulness": <1-7>}}"""

VERSUS = """You are a strict judge of creative ideas.

BRIEF
{brief}

IDEA A
{a}

IDEA B
{b}

Which idea is more creative, meaning both more novel (less like what a typical assistant would answer)
and more useful for the brief within its constraints? A merely strange idea does not beat a workable
one. Reply with JSON only: {{"winner": "A" or "B" or "tie"}}"""

GROUPS = """These ideas answer the same brief. Group together ideas that are essentially the same idea
(same core mechanism, even if worded differently). Every number must appear in exactly one group.

IDEAS
{ideas}

Reply with JSON only: {{"groups": [[<numbers>], ...]}}"""


def scale(value):
    try:
        v = float(value)
    except (TypeError, ValueError):
        return None
    return (v - 1) / 6 if 1 <= v <= 7 else None


def rate(prompt, text, args):
    reply = cb.chat(args.judge_model, [{"role": "user", "content": RATE.format(brief=brief_text(prompt), idea=text)}], 0.0)[0]
    parsed = jparse(reply) or {}
    n, u = scale(parsed.get("novelty")), scale(parsed.get("usefulness"))
    return None if n is None or u is None else {"novelty": n, "usefulness": u, "creative": n * u}


def versus(prompt, text, reference, args):
    """1, 0.5 or 0 for `text` against `reference`, averaged over both orders."""
    total = 0.0
    for first_is_text in (True, False):
        a, b = (text, reference) if first_is_text else (reference, text)
        reply = cb.chat(args.judge_model, [{"role": "user", "content": VERSUS.format(brief=brief_text(prompt), a=a, b=b)}], 0.0)[0]
        winner = str((jparse(reply) or {}).get("winner", "tie")).strip().upper()
        mine = "A" if first_is_text else "B"
        total += 1.0 if winner == mine else 0.5 if winner not in ("A", "B") else 0.0
    return total / 2


def cluster(texts, args):
    if len(texts) < 2:
        return [[i] for i in range(len(texts))]
    shown = "\n".join(f"[{i + 1}] {t}" for i, t in enumerate(texts))
    reply = cb.chat(args.judge_model, [{"role": "user", "content": GROUPS.format(ideas=shown)}], 0.0)[0]
    groups, seen = [], set()
    for g in (jparse(reply) or {}).get("groups", []):
        members = [int(n) - 1 for n in g if isinstance(n, (int, float)) and 1 <= int(n) <= len(texts) and int(n) - 1 not in seen]
        if members:
            groups.append(members)
            seen.update(members)
    groups += [[i] for i in range(len(texts)) if i not in seen]
    return groups


def cumulative_utility(creative, groups):
    """NoveltyBench-style: only the first idea of each group counts, with a patience discount."""
    first_seen, utility, k = set(), 0.0, 0
    group_of = {m: gi for gi, g in enumerate(groups) for m in g}
    for i, score in enumerate(creative):
        gi = group_of.get(i)
        if gi in first_seen or score is None:
            continue
        first_seen.add(gi)
        utility += score * PATIENCE**k
        k += 1
    return utility


def judge_job(prompt, row, reference_texts, args):
    texts = row["polished"]
    with ThreadPoolExecutor(max_workers=args.judge_workers) as pool:
        rated = list(pool.map(lambda t: rate(prompt, t, args), texts))
        refs = random.Random(f"{prompt['id']}{row['arm']}").sample(reference_texts, min(len(reference_texts), len(texts))) if reference_texts else []
        wins = list(pool.map(lambda pair: versus(prompt, *pair, args), [(t, refs[i % len(refs)]) for i, t in enumerate(texts)])) if refs and row["arm"] != "single" else []
        groups = cluster(texts, args)
    creative = [r["creative"] if r else None for r in rated]
    got = [c for c in creative if c is not None]
    return {
        "rated": rated, "creative": creative,
        "mean_creative": sum(got) / len(got) if got else None,
        "best_creative": max(got) if got else None,
        "winrate": sum(wins) / len(wins) if wins else None,
        "groups": groups, "distinct": len(groups),
        "cumulative_utility": cumulative_utility(creative, groups),
    }


# --- commands ----------------------------------------------------------------


def append(path, row):
    with LOCK, path.open("a") as fh:
        fh.write(json.dumps(row) + "\n")


def read_rows(path):
    return [json.loads(x) for x in path.read_text().splitlines() if x.strip()] if path.exists() else []


def cmd_run(args):
    prompts = json.loads(Path(args.prompts).read_text())[: args.limit or None]
    out = RUNS / args.tag
    out.mkdir(parents=True, exist_ok=True)
    (out / "config.json").write_text(json.dumps({k: v for k, v in vars(args).items() if k != "fn"}, indent=2))
    gen_path, res_path = out / "ideas.jsonl", out / "results.jsonl"
    have = {(r["prompt_id"], r["arm"]) for r in read_rows(gen_path)}
    jobs = [(p, a) for p in prompts for a in args.arms if (p["id"], a) not in have]
    print(f"generating {len(jobs)} jobs ({len(have)} done); model {args.model}, judge {args.judge_model}")

    def work(prompt, arm):
        started, meter = time.time(), Meter()
        row = {"prompt_id": prompt["id"], "domain": prompt["domain"], "arm": arm}
        try:
            ideas = generate(prompt, arm, args, meter)
            with ThreadPoolExecutor(max_workers=max(1, len(ideas))) as pool:
                polished = list(pool.map(lambda i: polish(i, args, meter), ideas))
            row.update(ideas=ideas, polished=polished)
        except Exception as err:
            row.update(ideas=[], polished=[], error=str(err)[:400])
        row.update(tokens=meter.tokens, calls=meter.calls, seconds=round(time.time() - started, 1))
        append(gen_path, row)
        return row

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        futs = [pool.submit(work, p, a) for p, a in jobs]
        for n, fut in enumerate(as_completed(futs), 1):
            r = fut.result()
            print(f"[{n}/{len(jobs)}] {r['arm']:<10} {r['prompt_id']:<14} {r['seconds']:>6}s ideas={len(r['ideas'])} {r.get('error', '')[:70]}")

    if args.skip_judge:
        return
    judge_all(args, prompts, out)
    print_report(read_rows(res_path))


def judge_all(args, prompts, out):
    by_id = {p["id"]: p for p in prompts}
    gen = read_rows(out / "ideas.jsonl")
    res_path = out / "results.jsonl"
    done = {(r["prompt_id"], r["arm"]) for r in read_rows(res_path)}
    refs = {r["prompt_id"]: r["polished"] for r in gen if r["arm"] == "single"}
    todo = [r for r in gen if (r["prompt_id"], r["arm"]) not in done and r["prompt_id"] in by_id and r["polished"]]
    print(f"judging {len(todo)} sets")

    def work(row):
        verdict = judge_job(by_id[row["prompt_id"]], row, refs.get(row["prompt_id"], []), args)
        append(res_path, {**{k: row[k] for k in ("prompt_id", "domain", "arm", "tokens", "calls", "seconds")}, **verdict})

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        list(pool.map(work, todo))


def mean(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else float("nan")


def bootstrap_diff(a, b, reps=4000, seed=1):
    """95% CI of mean(a - b) over prompts present in both."""
    keys = sorted(set(a) & set(b))
    diffs = [a[k] - b[k] for k in keys if a[k] is not None and b[k] is not None]
    if len(diffs) < 3:
        return float("nan"), float("nan"), float("nan"), len(diffs)
    rng = random.Random(seed)
    means = sorted(sum(rng.choice(diffs) for _ in diffs) / len(diffs) for _ in range(reps))
    return sum(diffs) / len(diffs), means[int(0.025 * reps)], means[int(0.975 * reps)], len(diffs)


def print_report(rows):
    print(f"\n{'arm':<11}{'prompts':>8}{'mean':>7}{'best':>7}{'winrate':>9}{'distinct':>9}{'cum.util':>9}{'tok/prompt':>11}{'s/prompt':>9}")
    for arm in ARMS:
        mine = [r for r in rows if r["arm"] == arm]
        if not mine:
            continue
        wr = mean(r["winrate"] for r in mine)
        print(f"{arm:<11}{len(mine):>8}{mean(r['mean_creative'] for r in mine):>7.3f}{mean(r['best_creative'] for r in mine):>7.3f}"
              f"{wr:>9.1%}{mean(r['distinct'] for r in mine):>9.2f}{mean(r['cumulative_utility'] for r in mine):>9.3f}"
              f"{mean(r['tokens'] for r in mine):>11.0f}{mean(r['seconds'] for r in mine):>9.0f}")
    print("\npaired difference by prompt, mean creative score (95% bootstrap CI):")
    for metric in ("mean_creative", "best_creative", "cumulative_utility"):
        print(f"  {metric}")
        for base in ("single", "sampled"):
            if not any(r["arm"] == base for r in rows):
                continue
            ref = {r["prompt_id"]: r[metric] for r in rows if r["arm"] == base}
            for arm in ARMS:
                if arm == base or not any(r["arm"] == arm for r in rows):
                    continue
                d, lo, hi, n = bootstrap_diff({r["prompt_id"]: r[metric] for r in rows if r["arm"] == arm}, ref)
                flag = "  *" if lo > 0 or hi < 0 else ""
                print(f"    {arm:<11} vs {base:<8} {d:+.3f}  [{lo:+.3f}, {hi:+.3f}]  n={n}{flag}")


def cmd_report(args):
    print_report(read_rows(RUNS / args.tag / "results.jsonl"))


def cmd_judge(args):
    prompts = json.loads(Path(args.prompts).read_text())
    out = RUNS / args.tag
    judge_all(args, prompts, out)
    print_report(read_rows(out / "results.jsonl"))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    def common(s):
        s.add_argument("--prompts", default=str(HERE / "prompts.json"))
        s.add_argument("--tag", required=True)
        s.add_argument("--workers", type=int, default=6)
        s.add_argument("--judge-workers", type=int, default=4)
        s.add_argument("--judge-model", default="google/gemini-3.8-flash")
        s.add_argument("--model", default=os.environ.get("TINYBACKROOMS_OPENHUMAN_MODEL", "deepseek/deepseek-v4.1-flash"))

    r = sub.add_parser("run")
    common(r)
    r.add_argument("--arms", nargs="+", choices=ARMS, default=list(ARMS))
    r.add_argument("--limit", type=int, default=0)
    r.add_argument("--n", type=int, default=5, help="final ideas per arm per prompt")
    r.add_argument("--variations", type=int, default=3)
    r.add_argument("--generations", type=int, default=3)
    r.add_argument("--items-per-variation", type=int, default=3)
    r.add_argument("--template", choices=("cli", "fugue", "student"), default="cli")
    r.add_argument("--temperature", type=float, default=1.0)
    r.add_argument("--wall-clock", type=int, default=900)
    r.add_argument("--seed", type=int, default=0)
    r.add_argument("--api", default=os.environ.get("TINYBACKROOMS_URL", "http://127.0.0.1:8082"))
    r.add_argument("--skip-judge", action="store_true")
    r.set_defaults(fn=cmd_run)

    j = sub.add_parser("judge")
    common(j)
    j.set_defaults(fn=cmd_judge)

    q = sub.add_parser("report")
    q.add_argument("--tag", required=True)
    q.set_defaults(fn=cmd_report)

    args = p.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
