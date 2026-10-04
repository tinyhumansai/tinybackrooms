#!/usr/bin/env python3
"""Run tinybackrooms against CreativityBench and compare it with plain prompting.

Standard library only. The benchmark's own prompt builders and judge are reused
from a CreativityBench checkout (never copied into this repo), with the model
transport swapped for OpenRouter.

Subcommands:
    setup    clone CreativityBench and download the 3.3K-task release
    sample   write a seeded, difficulty-stratified subset of the tasks
    run      generate answers (and judge them) for every arm
    report   print the comparison table for a run

Arms:
    static     the benchmark's own protocol: one call, temperature 0
    static-k   K independent calls at the run temperature (breadth, no dialogue)
    brainstorm tinybackrooms, one agent, K variations
    backrooms  tinybackrooms, two agents, K variations of G turns

The tinybackrooms arms need the API running (see README.md).
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
import types
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

HERE = Path(__file__).resolve().parent
CACHE = HERE / ".cache"
RUNS = HERE / "runs"
CB_DIR = Path(os.environ.get("CREATIVITYBENCH_DIR", CACHE / "CreativityBench"))
FULL_TASKS = Path(os.environ.get("CB_FULL_TASKS", CACHE / "tasks_full.json"))
DATASET_ID = "1C7PqRknVjdj4OBpVPT22Opo167lHNyjs"
OPENROUTER = "https://openrouter.ai/api/v1/chat/completions"
ARMS = ("static", "static-k", "brainstorm", "backrooms")
LOCK = threading.Lock()

ANSWER_SCHEMA = {
    "type": "object",
    "required": ["gold_entity", "gold_part", "how_to_use"],
    "properties": {
        "gold_entity": {
            "type": "string",
            "description": "Entity name, copied exactly (case-sensitive) from the scene.",
        },
        "gold_part": {
            "type": "string",
            "description": "Part name of that entity, copied exactly (case-sensitive).",
        },
        "how_to_use": {
            "type": "string",
            "description": "Detailed step-by-step instructions for using that part to do the task.",
        },
    },
}


# --- transport ---------------------------------------------------------------


def http_json(url, body, headers, timeout, retries=4):
    """POST JSON and return the decoded reply, retrying transient failures."""
    data = json.dumps(body).encode()
    last = None
    for attempt in range(retries):
        req = urllib.request.Request(url, data=data, headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as err:
            detail = err.read().decode(errors="replace")[:500]
            last = RuntimeError(f"HTTP {err.code}: {detail}")
            if err.code not in (408, 429, 500, 502, 503, 504):
                raise last from None
        except (urllib.error.URLError, TimeoutError, ConnectionError) as err:
            last = err
        time.sleep(2**attempt)
    raise RuntimeError(f"giving up on {url}: {last}")


def chat(model, messages, temperature, max_tokens=4096):
    """One OpenRouter chat completion; returns (text, usage)."""
    key = os.environ.get("OPENROUTER_API_KEY")
    if not key:
        sys.exit("OPENROUTER_API_KEY is not set")
    reply = http_json(
        OPENROUTER,
        {
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
        },
        {"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        timeout=300,
    )
    text = reply["choices"][0]["message"].get("content") or ""
    return text, reply.get("usage", {})


# --- CreativityBench shim ----------------------------------------------------


def load_cb(judge_model):
    """Import CreativityBench's utils/judge with OpenRouter as the model transport."""
    if not (CB_DIR / "evaluation" / "judge.py").exists():
        sys.exit(f"no CreativityBench checkout at {CB_DIR}; run `cb.py setup`")
    sys.path.insert(0, str(CB_DIR / "evaluation"))
    models = types.ModuleType("models")
    models.call_model = lambda *a, **k: sys.exit("benchmark transport is stubbed")
    tqdm = types.ModuleType("tqdm")
    tqdm.tqdm = lambda it=None, *a, **k: it
    sys.modules.update({"models": models, "tqdm": tqdm})
    import judge
    import utils

    def call_judge(prompt):
        text, _ = chat(judge_model, [{"role": "user", "content": prompt}], 0.0, 4096)
        parsed = utils.parse_json(text)
        return parsed if isinstance(parsed, dict) else {}

    judge._call_judge_llm = call_judge
    # The entity-lookup file only exists after running their task_creation stage;
    # without it the judge sees no physical-attribute hints for wrong answers.
    judge._get_part_attrs = lambda *_a, **_k: ({}, {})
    return utils, judge


def scene_brief(utils, task):
    """The scene as the benchmark's static prompt shows it, without its answer format."""
    blocks = [
        f"TASK:\n{task['task']}\n",
        f"ENVIRONMENT:\n{task['environment']}\n",
        "=== ENTITIES AVAILABLE (full descriptions) ===",
    ]
    blocks += [utils._fmt_entity(e) for e in task["entities"]]
    items = [i for i in task["items"] if i["interactable"] == "Yes"]
    if items:
        blocks.append("\n=== OTHER ITEMS IN SCENE ===\n")
        blocks += [f"  • {i['name']}: {i['description']}" for i in items]
    blocks.append(
        "\nFind the entity part that accomplishes the task in a creative, "
        "non-obvious but physically plausible way."
    )
    return "\n".join(blocks)


# --- arms --------------------------------------------------------------------


def answer_of(parsed):
    if not isinstance(parsed, dict):
        return None
    out = {k: str(parsed.get(k, "")).strip() for k in ANSWER_SCHEMA["required"]}
    return out if any(out.values()) else None


def run_static(utils, task, args, temperature):
    text, usage = chat(
        args.model,
        [
            {"role": "system", "content": utils.SYS_STATIC},
            {"role": "user", "content": utils.build_static_prompt(task)},
        ],
        temperature,
    )
    _, parsed = utils.parse_response(text)
    return answer_of(parsed), {
        "input_tokens": usage.get("prompt_tokens", 0),
        "output_tokens": usage.get("completion_tokens", 0),
        "calls": 1,
    }


def arm_static(utils, task, args):
    answer, usage = run_static(utils, task, args, 0.0)
    return {"items": [{"value": answer}] if answer else [], "usage": usage}


def arm_static_k(utils, task, args):
    answers, usage = [], {"input_tokens": 0, "output_tokens": 0, "calls": 0}
    for _ in range(args.variations):
        try:
            answer, spent = run_static(utils, task, args, args.temperature)
        except RuntimeError:
            continue
        for k, v in spent.items():
            usage[k] += v
        if answer:
            answers.append({"value": answer})
    return {"items": answers, "usage": usage}


def arm_tinybackrooms(utils, task, args, mode):
    request = {
        "input": {
            "brief": scene_brief(utils, task),
            "constraints": {
                "must": [
                    "gold_entity and gold_part must be copied exactly from the scene",
                    "use only entities and items that are in the scene; never invent tools",
                ],
                "must_not": ["rely on tools or materials that are not listed"],
            },
        },
        "output": {
            "item_name": "answer",
            "item_guidance": "One answer: the entity, its part, and how to use it.",
            "schema": ANSWER_SCHEMA,
        },
        "config": {
            "variations": args.variations,
            "generations": args.generations,
            "items_per_variation": 1,
            "mode": mode,
            "template": args.template,
            "temperature": args.temperature,
            "seed": args.seed,
            "scoring": {
                "scorers": [{"kind": "judge", "weight": 1.0}],
                "select_top_k": args.variations,
                "judge_raters": args.judge_raters,
            },
            "budget": {"wall_clock_secs": args.wall_clock},
        },
    }
    reply = http_json(
        f"{args.api}/v1/generate",
        request,
        {"Content-Type": "application/json"},
        timeout=args.wall_clock + 120,
        retries=1,
    )
    return {
        "items": [
            {"value": i["value"], "total": i.get("total"), "scores": i.get("scores")}
            for i in reply["items"]
        ],
        "usage": reply.get("usage", {}),
        "diversity": reply.get("diversity"),
        "warnings": reply.get("warnings", []),
    }


def generate(utils, task, arm, args):
    if arm == "static":
        return arm_static(utils, task, args)
    if arm == "static-k":
        return arm_static_k(utils, task, args)
    return arm_tinybackrooms(utils, task, args, arm)


# --- judging -----------------------------------------------------------------


def judge_items(judge, task, items):
    """Attach CreativityBench's verdict to every item, in place."""
    for item in items:
        value = item["value"]
        try:
            verdict = judge._judge_one(task, value)
        except Exception as err:  # a failed judgement is a data point, not a crash
            item["judge_error"] = str(err)[:300]
            continue
        llm = verdict.get("llm_judge", {})
        item["verdict"] = {
            "gold": bool(verdict["entity_correct"] and verdict["part_correct"]),
            "entity_correct": bool(verdict["entity_correct"]),
            "relation": verdict["prediction_relation"],
            "correctness": llm.get("prediction_correctness", 0),
            "grounding": llm.get("attributes_grounding", 0),
            "feasibility": llm.get("action_feasibility", 0),
            "reasonable": (verdict.get("prediction_reasonability_judge") or {}).get(
                "prediction_reasonable_level"
            ),
            "pair": [verdict["best_matching_entity"], verdict["best_matching_part"]],
        }


# --- commands ----------------------------------------------------------------


def cmd_setup(_args):
    CACHE.mkdir(exist_ok=True)
    if not CB_DIR.exists():
        os.system(f"git clone --depth 1 https://github.com/CreativityBench/CreativityBench '{CB_DIR}'")
    if FULL_TASKS.exists():
        print(f"{FULL_TASKS} already present")
        return
    base = f"https://drive.google.com/uc?export=download&id={DATASET_ID}"
    page = urllib.request.urlopen(base).read().decode(errors="replace")
    action = re.search(r'action="([^"]+)"', page).group(1)
    fields = dict(re.findall(r'name="([^"]+)" value="([^"]*)"', page))
    url = f"{action}?{urllib.parse.urlencode(fields)}"
    print("downloading the task release (about 400 MB)…")
    urllib.request.urlretrieve(url, FULL_TASKS)
    print(f"saved {FULL_TASKS}")


def cmd_sample(args):
    tasks = json.loads(Path(args.source).read_text())
    rng = random.Random(args.seed)
    by_level = {}
    for t in tasks:
        by_level.setdefault(t["setting"]["difficulty"], []).append(t)
    chosen = []
    for level in sorted(by_level):
        pool = by_level[level]
        chosen += rng.sample(pool, min(args.per_difficulty, len(pool)))
    Path(args.out).write_text(json.dumps(chosen))
    counts = {k: min(args.per_difficulty, len(v)) for k, v in by_level.items()}
    print(f"wrote {len(chosen)} tasks to {args.out} {counts}")


def append(path, row):
    with LOCK, path.open("a") as fh:
        fh.write(json.dumps(row) + "\n")


def read_rows(path):
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def cmd_run(args):
    utils, judge = load_cb(args.judge_model)
    tasks = json.loads(Path(args.tasks).read_text())[: args.limit or None]
    out = RUNS / args.tag
    out.mkdir(parents=True, exist_ok=True)
    config = {k: v for k, v in vars(args).items() if k != "fn"}
    (out / "config.json").write_text(json.dumps(config, indent=2))
    path = out / "results.jsonl"
    done = {(r["task_id"], r["arm"]) for r in read_rows(path)}
    jobs = [(t, a) for t in tasks for a in args.arms if (t["task_id"], a) not in done]
    print(f"{len(jobs)} jobs ({len(done)} already done), model {args.model}")

    def work(task, arm):
        started = time.time()
        row = {"task_id": task["task_id"], "arm": arm, "difficulty": task["setting"]["difficulty"]}
        try:
            row.update(generate(utils, task, arm, args))
            judge_items(judge, task, row["items"])
        except Exception as err:
            row.update(items=[], error=str(err)[:500])
        row["seconds"] = round(time.time() - started, 1)
        append(path, row)
        return row

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = [pool.submit(work, t, a) for t, a in jobs]
        for n, fut in enumerate(as_completed(futures), 1):
            row = fut.result()
            top = (row["items"][:1] or [{}])[0].get("verdict", {})
            print(
                f"[{n}/{len(jobs)}] {row['arm']:<10} {row['difficulty']:<9} "
                f"{row['seconds']:>6}s items={len(row['items'])} "
                f"gold={top.get('gold')} {row.get('error', '')[:80]}"
            )
    print_report(read_rows(path))


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else float("nan")


def quality(v):
    parts = [v.get(k) for k in ("correctness", "grounding", "feasibility")]
    parts = [p for p in parts if isinstance(p, (int, float))]
    return mean(p / 2 for p in parts) if parts else float("nan")


def accepted(v):
    return v["gold"] or str(v["relation"]).startswith("similar")


def summarize(rows):
    table = {}
    for arm in ARMS:
        mine = [r for r in rows if r["arm"] == arm]
        if not mine:
            continue
        judged = [[i["verdict"] for i in r["items"] if "verdict" in i] for r in mine]
        have = [j for j in judged if j]
        top = [j[0] for j in have]
        table[arm] = {
            "tasks": len(mine),
            "errors": sum(1 for r in mine if r.get("error")),
            "top1_gold": mean(v["gold"] for v in top),
            "any_gold": mean(any(v["gold"] for v in j) for j in have),
            "mean_gold": mean(mean(v["gold"] for v in j) for j in have),
            "top1_accept": mean(accepted(v) for v in top),
            "top1_quality": mean(quality(v) for v in top),
            "distinct": mean(len({tuple(v["pair"]) for v in j}) / len(j) for j in have),
            "tokens": mean(
                (r.get("usage") or {}).get("input_tokens", 0)
                + (r.get("usage") or {}).get("output_tokens", 0)
                for r in mine
            ),
            "seconds": mean(r["seconds"] for r in mine),
        }
    return table


COLUMNS = [
    ("tasks", "tasks", "{:>5}"),
    ("errors", "err", "{:>4}"),
    ("top1_gold", "top1 gold", "{:>9.1%}"),
    ("any_gold", "any@K gold", "{:>10.1%}"),
    ("mean_gold", "mean gold", "{:>9.1%}"),
    ("top1_accept", "top1 ok", "{:>8.1%}"),
    ("top1_quality", "quality", "{:>8.2f}"),
    ("distinct", "distinct", "{:>8.1%}"),
    ("tokens", "tok/task", "{:>9.0f}"),
    ("seconds", "s/task", "{:>7.1f}"),
]


def print_report(rows):
    table = summarize(rows)
    print(f"\n{'arm':<11}" + " ".join(f"{h:>{len(f.format(0))}}" for _, h, f in COLUMNS))
    for arm, stats in table.items():
        print(f"{arm:<11}" + " ".join(f.format(stats[k]) for k, _, f in COLUMNS))
    levels = sorted({r["difficulty"] for r in rows})
    for level in levels:
        sub = summarize([r for r in rows if r["difficulty"] == level])
        line = "  ".join(f"{a}={s['top1_gold']:.0%}" for a, s in sub.items())
        print(f"  top1 gold @ {level:<9} {line}")


def cmd_report(args):
    print_report(read_rows(RUNS / args.tag / "results.jsonl"))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("setup").set_defaults(fn=cmd_setup)

    s = sub.add_parser("sample")
    s.add_argument("--source", default=str(FULL_TASKS))
    s.add_argument("--out", required=True)
    s.add_argument("--per-difficulty", type=int, default=20)
    s.add_argument("--seed", type=int, default=7)
    s.set_defaults(fn=cmd_sample)

    r = sub.add_parser("run")
    r.add_argument("--tasks", required=True)
    r.add_argument("--tag", required=True)
    r.add_argument("--arms", nargs="+", choices=ARMS, default=list(ARMS))
    r.add_argument("--limit", type=int, default=0)
    r.add_argument("--workers", type=int, default=4)
    r.add_argument("--model", default=os.environ.get("TINYBACKROOMS_OPENHUMAN_MODEL", "deepseek/deepseek-v4.1-flash"))
    r.add_argument("--judge-model", default="google/gemini-3.8-flash")
    r.add_argument("--api", default=os.environ.get("TINYBACKROOMS_URL", "http://127.0.0.1:8080"))
    r.add_argument("--variations", type=int, default=3)
    r.add_argument("--generations", type=int, default=4)
    r.add_argument("--judge-raters", type=int, default=1)
    r.add_argument("--template", choices=("cli", "fugue", "student"), default="cli")
    r.add_argument("--temperature", type=float, default=1.0)
    r.add_argument("--wall-clock", type=int, default=600)
    r.add_argument("--seed", type=int, default=0)
    r.set_defaults(fn=cmd_run)

    q = sub.add_parser("report")
    q.add_argument("--tag", required=True)
    q.set_defaults(fn=cmd_report)

    args = p.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
