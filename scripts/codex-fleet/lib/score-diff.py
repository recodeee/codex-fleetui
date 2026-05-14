#!/usr/bin/env python3
"""Single-purpose diff scorer: ask Claude whether a diff satisfies a plan's
acceptance criteria, return a JSON verdict on stdout.

This is the shared primitive behind two surfaces:
- Improvement #1 (post-merge quality rail): the merged-PR wrapper
  (`score-merged-pr.sh`) calls this, writes the result into
  `/tmp/claude-viz/fleet-quality-scores.json`, and the fleet-data::scores
  reader feeds it into WorkerRow.quality so fleet-state's third rail
  shows it.
- Improvement #3 (critic-at-checkpoint): the checkpoint wrapper
  (`score-checkpoint.sh`) calls this with the *uncommitted* diff from an
  active agent worktree, writes the result into
  `/tmp/claude-viz/fleet-checkpoint-warnings.json`. A low score signals
  the agent has drifted off-plan mid-task.

Same call shape, same prompt, different inputs and different sinks —
exactly the sequencing argued in conversation: build the scorer once,
reuse it for #1 post-merge and #3 at checkpoints.

## Input

JSON on stdin:
{
  "diff": "...full unified diff text...",
  "criteria": "...the Acceptance Criteria block from plan.md...",
  "mode": "merged" | "checkpoint",
  "pr_title": "..."  // optional, helps the model
}

## Output

JSON on stdout. The exact shape `fleet-data::scores` expects:
{
  "score": 87 | null,
  "criteria_met": ["..."],
  "criteria_missed": ["..."],
  "reasoning": "..."
}

`score` is `null` when the criteria list is empty (no plan found, or a
hotfix). The caller folds this into the higher-level scores file with the
agent-id, PR number, branch, etc. — those are not the scorer's concern.

## Failure posture

- Missing `ANTHROPIC_API_KEY` → exits non-zero with a brief stderr.
- API call failures (network, 5xx) → exits non-zero with stderr; the
  wrapper script decides whether to retry or skip.
- The model returns non-JSON → we strip prose and try again; if still
  no JSON, exit non-zero. The wrapper treats that as "no score this
  cycle" rather than overwriting a prior good score with garbage.

## Why stdlib only

The codex-fleet repo has no Python dependency surface anywhere — every
existing script is bash. Pip is not a given. Using only urllib + json
keeps that contract: drop the file in `lib/`, chmod +x, it runs.
"""

from __future__ import annotations

import json
import os
import re
import sys
import urllib.error
import urllib.request

ENDPOINT = "https://api.anthropic.com/v1/messages"
MODEL = "claude-sonnet-4-6"
ANTHROPIC_VERSION = "2023-06-01"
MAX_TOKENS = 1024

SYSTEM_PROMPT = """You review whether a diff satisfies the acceptance criteria of a plan.

Rules:
- A criterion counts as "met" only if the diff demonstrably implements it.
- A criterion is "missed" if you cannot trace it to a code change in the diff.
- If the diff does work the criteria do not cover, DO NOT penalize — you are
  scoring satisfaction of the listed criteria, not the breadth of the diff.
- Be strict. Vague matches do not count.

Output strictly JSON with this shape, no other text:
{
  "score": <integer 0-100, or null if criteria list is empty>,
  "criteria_met":    ["criterion text 1", "criterion text 2"],
  "criteria_missed": ["criterion text 3"],
  "reasoning": "<1-3 sentences>"
}
"""


def build_user_message(payload: dict) -> str:
    criteria = (payload.get("criteria") or "").strip()
    diff = (payload.get("diff") or "").strip()
    title = (payload.get("pr_title") or "").strip()
    mode = payload.get("mode", "merged")

    if not criteria:
        # Empty criteria → caller wants a null score; spell it out for the
        # model so it doesn't hallucinate criteria.
        return (
            f"MODE: {mode}\n"
            f"PR TITLE: {title or '(none)'}\n\n"
            "ACCEPTANCE CRITERIA: (none — no plan.md found for this branch)\n\n"
            "Return score: null with reasoning explaining why.\n"
        )

    return (
        f"MODE: {mode}\n"
        f"PR TITLE: {title or '(none)'}\n\n"
        f"ACCEPTANCE CRITERIA:\n{criteria}\n\n"
        f"DIFF:\n{diff}\n"
    )


def call_model(user_message: str, api_key: str) -> str:
    body = {
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "system": SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": user_message}],
    }
    req = urllib.request.Request(
        ENDPOINT,
        data=json.dumps(body).encode("utf-8"),
        headers={
            "x-api-key": api_key,
            "anthropic-version": ANTHROPIC_VERSION,
            "content-type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=90) as resp:
        payload = json.loads(resp.read().decode("utf-8"))
    # Anthropic Messages API: content is a list of blocks; the first text
    # block is the model's reply for non-tool runs.
    for block in payload.get("content", []):
        if block.get("type") == "text":
            return block.get("text", "")
    return ""


def extract_json(text: str) -> dict | None:
    """Pull the JSON object out of the model's reply.

    The system prompt asks for strict JSON, but models occasionally wrap
    it in markdown fences or prose. Try direct parse first, then strip
    common decorations, then look for the first balanced {...} block.
    """
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass

    # Strip ```json ... ``` fences.
    fence = re.search(r"```(?:json)?\s*(\{.*?\})\s*```", text, re.DOTALL)
    if fence:
        try:
            return json.loads(fence.group(1))
        except json.JSONDecodeError:
            pass

    # First balanced top-level {...}.
    start = text.find("{")
    if start == -1:
        return None
    depth = 0
    for i in range(start, len(text)):
        c = text[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                try:
                    return json.loads(text[start : i + 1])
                except json.JSONDecodeError:
                    return None
    return None


def normalize(parsed: dict) -> dict:
    """Coerce the model's output into the exact shape callers expect."""
    score = parsed.get("score")
    if isinstance(score, bool):  # JSON booleans pass `isinstance(_, int)`
        score = None
    if isinstance(score, (int, float)):
        score = int(max(0, min(100, score)))
    else:
        score = None
    return {
        "score": score,
        "criteria_met": [str(c) for c in parsed.get("criteria_met", [])][:50],
        "criteria_missed": [str(c) for c in parsed.get("criteria_missed", [])][:50],
        "reasoning": str(parsed.get("reasoning", ""))[:1000],
    }


def main() -> int:
    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        sys.stderr.write(
            "score-diff.py: ANTHROPIC_API_KEY is not set — cannot score.\n"
        )
        return 2

    try:
        payload = json.loads(sys.stdin.read())
    except json.JSONDecodeError as e:
        sys.stderr.write(f"score-diff.py: stdin is not JSON: {e}\n")
        return 2

    user_message = build_user_message(payload)
    try:
        text = call_model(user_message, api_key)
    except urllib.error.HTTPError as e:
        sys.stderr.write(f"score-diff.py: API HTTP error {e.code}: {e.read().decode('utf-8', 'replace')}\n")
        return 3
    except urllib.error.URLError as e:
        sys.stderr.write(f"score-diff.py: API network error: {e}\n")
        return 3

    parsed = extract_json(text)
    if not parsed:
        sys.stderr.write(
            "score-diff.py: model reply contained no parseable JSON.\n"
            f"raw reply: {text[:500]}\n"
        )
        return 4

    json.dump(normalize(parsed), sys.stdout)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
