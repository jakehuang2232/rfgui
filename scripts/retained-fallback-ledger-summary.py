#!/usr/bin/env python3
"""Summarize a RetainedAuto fallback ledger.

Temporary survey tool for retiring the Legacy renderer; delete it together
with `src/view/viewport/render/fallback_ledger.rs`.

Usage:
    RFGUI_RETAINED_FALLBACK_LEDGER=/path/ledger.jsonl <run tests or the app>
    python3 -I scripts/retained-fallback-ledger-summary.py /path/ledger.jsonl

Frames are grouped by signature: fallback stage, terminal stage, normalized
candidate rejections and authority detail, any frame graph failure, and the
set of per-owner fallback records. Numbers inside Debug output (node keys,
surface ids, chunk indices) are normalized to `#`.
"""

import collections
import json
import re
import sys

NUMBER = re.compile(r"\d+")


def normalize(text):
    return NUMBER.sub("#", text)


def field(authority, name):
    """Extract `name=[...]` or `name=value` from the authority trace."""
    start = authority.find(name + "=")
    if start < 0:
        return ""
    start += len(name) + 1
    if start < len(authority) and authority[start] == "[":
        depth = 0
        for index in range(start, len(authority)):
            if authority[index] == "[":
                depth += 1
            elif authority[index] == "]":
                depth -= 1
                if depth == 0:
                    return authority[start + 1 : index]
        return authority[start + 1 :]
    end = authority.find(" ", start)
    return authority[start:] if end < 0 else authority[start:end]


def signature(entry):
    authority = entry.get("authority", "")
    fallbacks = sorted(
        {
            f"{item['category']}:{item['detail']}@{item['element']}"
            for item in entry.get("fallbacks", [])
        }
    )
    return (
        entry.get("legacy_fallback_stage", "?"),
        entry.get("terminal_failure_stage", "?"),
        normalize(field(authority, "candidate-rejections")),
        normalize(field(authority, "detail")),
        normalize(entry.get("graph_failure", "")),
        tuple(fallbacks),
    )


def context(entry):
    return f"{entry.get('process', '?')} :: {entry.get('thread', '?')}"


def main(path):
    frames = collections.Counter()
    contexts = collections.defaultdict(set)
    per_context = collections.Counter()
    malformed = 0
    with open(path, encoding="utf-8") as ledger:
        for line in ledger:
            line = line.strip()
            if not line:
                continue
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                malformed += 1
                continue
            key = signature(entry)
            frames[key] += 1
            contexts[key].add(context(entry))
            per_context[context(entry)] += 1

    total = sum(frames.values())
    print(f"fallback frames: {total}  signatures: {len(frames)}  "
          f"contexts: {len(per_context)}  malformed lines: {malformed}")
    print()
    for rank, (key, count) in enumerate(frames.most_common(), start=1):
        stage, terminal, rejections, detail, graph, fallbacks = key
        print(f"#{rank}  frames={count}  contexts={len(contexts[key])}")
        print(f"    stage={stage} terminal={terminal}")
        if rejections:
            print(f"    rejections: {rejections}")
        if detail:
            print(f"    detail: {detail}")
        if graph:
            print(f"    graph failure: {graph}")
        for item in fallbacks:
            print(f"    fallback: {item}")
        for name in sorted(contexts[key])[:5]:
            print(f"    seen in: {name}")
        if len(contexts[key]) > 5:
            print(f"    ... {len(contexts[key]) - 5} more contexts")
        print()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
