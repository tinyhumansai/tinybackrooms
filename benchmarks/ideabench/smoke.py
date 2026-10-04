#!/usr/bin/env python3
"""Quick IdeaBench check against the plain-prompting baseline, on DeepSeek V4.1 Flash.

    python3 benchmarks/ideabench/smoke.py            # smoke: 6 briefs, a few minutes
    python3 benchmarks/ideabench/smoke.py --full --yes   # all 32 briefs, every arm

It starts its own tinybackrooms API on a private port with DeepSeek as the
generator, runs `ib.py`, prints the report (every arm against `single`, the
baseline), and stops the API. Needs OPENROUTER_API_KEY and a release build:

    cargo build --release -p tinybackrooms-api --features openhuman

The judge stays a different family (Gemini) so the generator never grades
itself. Use `--dry-run` to see what would run without spending anything.
"""

from __future__ import annotations

import argparse
import json
import os
import signal
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
BINARY = ROOT / "target" / "release" / "tinybackrooms-api"
MODEL = "deepseek/deepseek-v4.1-flash"
JUDGE = "google/gemini-3.8-flash"
PORT = 8091

# One brief from each of six domains, AUT included: broad, not deep.
SMOKE_IDS = ["product-01", "science-04", "game-09", "civic-12", "art-22", "aut-26"]

PRESETS = {
    "smoke": {
        "arms": ["single", "sampled", "brainstorm", "backrooms"],
        "variations": 2, "generations": 2, "items": 3, "workers": 8,
        "estimate": "about 15 minutes and 3-5M tokens",
    },
    "full": {
        "arms": ["single", "sampled", "brainstorm", "backrooms", "evolve"],
        "variations": 3, "generations": 3, "items": 3, "workers": 12,
        "estimate": "several hours and roughly 30-60M tokens",
    },
}


def wait_healthy(deadline_s=30):
    end = time.time() + deadline_s
    while time.time() < end:
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{PORT}/healthz", timeout=2)
            return True
        except OSError:
            time.sleep(0.5)
    return False


def start_api():
    env = {
        **os.environ,
        "TINYBACKROOMS_ADDR": f"127.0.0.1:{PORT}",
        "TINYBACKROOMS_BACKEND": "openhuman",
        "TINYBACKROOMS_OPENHUMAN_API_KEY": os.environ["OPENROUTER_API_KEY"],
        "TINYBACKROOMS_OPENHUMAN_BASE_URL": "https://openrouter.ai/api/v1",
        "TINYBACKROOMS_OPENHUMAN_MODEL": MODEL,
        "TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS": "240",
    }
    log = open(HERE / "runs" / "smoke-api.log", "w")
    proc = subprocess.Popen([str(BINARY)], env=env, stdout=log, stderr=subprocess.STDOUT)
    if not wait_healthy():
        proc.kill()
        sys.exit(f"the API did not come up on port {PORT}; see runs/smoke-api.log")
    return proc


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("--full", action="store_true", help="all briefs and every arm")
    ap.add_argument("--yes", action="store_true", help="confirm a --full run, which spends real money")
    ap.add_argument("--tag", help="run name (default: smoke-<time>)")
    ap.add_argument("--arms", nargs="+", help="override the preset's arms")
    ap.add_argument("--prompts", help="a briefs file to use instead of prompts.json (every brief runs)")
    ap.add_argument("--judge-model", default=JUDGE)
    ap.add_argument("--dry-run", action="store_true", help="print the plan and stop")
    args = ap.parse_args()

    mode = "full" if args.full else "smoke"
    preset = PRESETS[mode]
    arms = args.arms or preset["arms"]
    tag = args.tag or f"{mode}-{time.strftime('%m%d-%H%M')}"
    (HERE / "runs").mkdir(exist_ok=True)

    prompts = Path(args.prompts).resolve() if args.prompts else HERE / "prompts.json"
    if mode == "smoke" and not args.prompts:
        every = json.loads(prompts.read_text())
        chosen = [p for p in every if p["id"] in SMOKE_IDS]
        prompts = HERE / "runs" / "smoke-prompts.json"
        prompts.write_text(json.dumps(chosen))
    count = len(json.loads(prompts.read_text()))

    cmd = [
        sys.executable, "-u", str(HERE / "ib.py"), "run", "--tag", tag, "--prompts", str(prompts),
        "--model", MODEL, "--judge-model", args.judge_model, "--api", f"http://127.0.0.1:{PORT}",
        "--arms", *arms, "--n", "5", "--variations", str(preset["variations"]),
        "--generations", str(preset["generations"]), "--items-per-variation", str(preset["items"]),
        "--workers", str(preset["workers"]),
    ]
    print(f"mode {mode}: {count} briefs x {len(arms)} arms ({', '.join(arms)}), generator {MODEL}, "
          f"judge {args.judge_model}\nexpect {preset['estimate']}; run tag {tag}")
    if args.dry_run:
        print("would run:", " ".join(cmd))
        return
    if mode == "full" and not args.yes:
        sys.exit("a full run spends real money; add --yes to confirm")
    if not os.environ.get("OPENROUTER_API_KEY"):
        sys.exit("OPENROUTER_API_KEY is not set")
    if not BINARY.exists():
        sys.exit(f"missing {BINARY}; build it with:\n  cargo build --release -p tinybackrooms-api --features openhuman")

    api = start_api()
    try:
        status = subprocess.call(cmd)
    finally:
        api.send_signal(signal.SIGINT)
        try:
            api.wait(timeout=10)
        except subprocess.TimeoutExpired:
            api.kill()
    print(f"\nresults: benchmarks/ideabench/runs/{tag}/   (re-print with: ib.py report --tag {tag})")
    sys.exit(status)


if __name__ == "__main__":
    main()
