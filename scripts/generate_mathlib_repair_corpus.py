"""Generate a Mathlib-derived Lean proof-repair corpus.

The generator is intentionally conservative: it only uses Mathlib declarations
that already contain an explicit `:= by` proof. The broken program replaces
that proof with an unknown identifier, while the expected patch restores the
original Mathlib proof text. This yields deterministic LSP-shaped patches and
keeps the fixed target tied to a specific Mathlib commit.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
import re
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


CANDIDATE_RE = re.compile(
    r"^(?P<prefix>(?:private|protected)\s+)?(?P<kind>theorem|lemma|example)\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_.'\u207f\u2080-\u2089]*)?"
)
TOPLEVEL_RE = re.compile(
    r"^(?:/--|/-!|@\[\s*|attribute\b|open\b|variable\b|universe\b|"
    r"(?:private\s+|protected\s+|noncomputable\s+|unsafe\s+|partial\s+)*"
    r"(?:theorem|lemma|example|def|abbrev|instance|class|structure|inductive|"
    r"axiom|constant|opaque|namespace|section|end)\b)"
)
MUTATED_PROOF = "by\n  exact __refineforge_missing_proof__"
DIAGNOSTIC = "unknown identifier `__refineforge_missing_proof__`"


@dataclass(frozen=True)
class Candidate:
    source_path: str
    declaration_kind: str
    declaration_name: str
    start_line: int
    end_line: int
    original: str
    broken: str
    patch: dict[str, object]


def _run_git(root: Path, *args: str) -> str | None:
    try:
        out = subprocess.check_output(
            ["git", "-C", str(root), *args],
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except Exception:
        return None
    return out.strip() or None


def _line_col(text: str, offset: int) -> tuple[int, int]:
    prefix = text[:offset]
    line = prefix.count("\n")
    last_newline = prefix.rfind("\n")
    char = len(prefix) if last_newline < 0 else len(prefix) - last_newline - 1
    return line, char


def _decl_starts(lines: list[str]) -> list[int]:
    starts: list[int] = []
    for i, line in enumerate(lines):
        if TOPLEVEL_RE.match(line):
            starts.append(i)
    return starts


def _extract_from_file(
    mathlib_root: Path,
    path: Path,
    *,
    max_proof_chars: int,
) -> Iterable[Candidate]:
    rel = path.relative_to(mathlib_root).as_posix()
    text = path.read_text(encoding="utf-8", errors="ignore")
    lines = text.splitlines(keepends=True)
    starts = _decl_starts(lines)
    if not starts:
        return
    offsets: list[int] = []
    cursor = 0
    for line in lines:
        offsets.append(cursor)
        cursor += len(line)

    for pos, start in enumerate(starts):
        line = lines[start]
        match = CANDIDATE_RE.match(line)
        if match is None:
            continue
        end = starts[pos + 1] if pos + 1 < len(starts) else len(lines)
        block = "".join(lines[start:end]).rstrip()
        proof_marker = ":= by"
        marker = block.find(proof_marker)
        if marker < 0:
            continue
        proof_start = marker + len(":= ")
        original_proof = block[proof_start:].rstrip()
        if "\nsorry" in original_proof or "\nadmit" in original_proof:
            continue
        if len(original_proof.splitlines()) > 40:
            continue
        if len(original_proof) > max_proof_chars:
            continue
        broken = block[:proof_start] + MUTATED_PROOF
        patch_start_line, patch_start_char = _line_col(broken, proof_start)
        patch_end_line, patch_end_char = _line_col(broken, len(broken))
        name = match.group("name") or f"anonymous_line_{start + 1}"
        yield Candidate(
            source_path=rel,
            declaration_kind=match.group("kind"),
            declaration_name=name,
            start_line=start + 1,
            end_line=end,
            original=block,
            broken=broken,
            patch={
                "start_line": patch_start_line,
                "start_char": patch_start_char,
                "end_line": patch_end_line,
                "end_char": patch_end_char,
                "new_text": original_proof,
            },
        )


def collect_candidates(mathlib_root: Path, *, max_proof_chars: int = 1200) -> list[Candidate]:
    source_root = mathlib_root / "Mathlib"
    candidates: list[Candidate] = []
    for path in sorted(source_root.rglob("*.lean")):
        candidates.extend(_extract_from_file(mathlib_root, path, max_proof_chars=max_proof_chars))
    return candidates


def _entry_id(commit: str, candidate: Candidate, index: int) -> str:
    raw = f"{commit}:{candidate.source_path}:{candidate.declaration_name}:{index}"
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()[:12]
    return f"mathlib4-{commit[:8]}-{index:06d}-{digest}"


def _make_entry(
    candidate: Candidate,
    *,
    commit: str,
    repo: str,
    index: int,
    split: str,
) -> dict[str, object]:
    prompt = (
        "Diagnostic: "
        + DIAGNOSTIC
        + "\nSource:\n```lean\n"
        + candidate.broken
        + "\n```"
    )
    return {
        "id": _entry_id(commit, candidate, index),
        "split": split,
        "source": {
            "repo": repo,
            "commit": commit,
            "path": candidate.source_path,
            "declaration_kind": candidate.declaration_kind,
            "declaration_name": candidate.declaration_name,
            "start_line": candidate.start_line,
            "end_line": candidate.end_line,
        },
        "mutation": {
            "kind": "replace_proof_with_unknown_identifier",
            "diagnostic": DIAGNOSTIC,
        },
        "diagnostic": DIAGNOSTIC,
        "prompt": prompt,
        "broken_lean": candidate.broken,
        "fixed_lean": candidate.original,
        "expected_patch": candidate.patch,
    }


def _write_jsonl(path: Path, rows: list[dict[str, object]]) -> None:
    with path.open("w", encoding="utf-8", newline="\n") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")


def _assign_splits(total: int, train_ratio: float, val_ratio: float) -> list[str]:
    train_count = int(total * train_ratio)
    val_count = int(total * val_ratio)
    if total >= 3:
        train_count = max(1, train_count)
        val_count = max(1, val_count)
    if train_count + val_count >= total:
        val_count = max(0, total - train_count - 1)
    held_count = total - train_count - val_count
    return ["train"] * train_count + ["val"] * val_count + ["heldout"] * held_count


def generate_corpus(
    *,
    mathlib_root: Path,
    out_dir: Path,
    limit: int,
    seed: int,
    source_commit: str | None = None,
    source_repo: str = "https://github.com/leanprover-community/mathlib4",
    train_ratio: float = 0.8,
    val_ratio: float = 0.1,
    max_proof_chars: int = 1200,
) -> dict[str, object]:
    mathlib_root = mathlib_root.resolve()
    if not (mathlib_root / "Mathlib").exists():
        raise FileNotFoundError(f"{mathlib_root} does not contain Mathlib/")
    commit = source_commit or _run_git(mathlib_root, "rev-parse", "HEAD") or "unknown"
    candidates = collect_candidates(mathlib_root, max_proof_chars=max_proof_chars)
    if len(candidates) < limit:
        raise ValueError(
            f"requested {limit} examples but only found {len(candidates)} usable declarations"
        )
    rng = random.Random(seed)
    rng.shuffle(candidates)
    selected = candidates[:limit]
    split_names = _assign_splits(len(selected), train_ratio, val_ratio)
    rows = [
        _make_entry(
            candidate,
            commit=commit,
            repo=source_repo,
            index=i + 1,
            split=split,
        )
        for i, (candidate, split) in enumerate(zip(selected, split_names, strict=True))
    ]

    out_dir.mkdir(parents=True, exist_ok=True)
    _write_jsonl(out_dir / "all.jsonl", rows)
    for split in ("train", "val", "heldout"):
        _write_jsonl(out_dir / f"{split}.jsonl", [r for r in rows if r["split"] == split])
    manifest = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "generator": Path(__file__).name,
        "source_repo": source_repo,
        "source_commit": commit,
        "mutation": "replace_proof_with_unknown_identifier",
        "diagnostic": DIAGNOSTIC,
        "total_examples": len(rows),
        "total_candidates_seen": len(candidates),
        "max_proof_chars": max_proof_chars,
        "seed": seed,
        "splits": {
            split: sum(1 for r in rows if r["split"] == split)
            for split in ("train", "val", "heldout")
        },
        "files": {
            "all": "all.jsonl",
            "train": "train.jsonl",
            "val": "val.jsonl",
            "heldout": "heldout.jsonl",
        },
    }
    (out_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return {"written": len(rows), "out_dir": str(out_dir), "manifest": manifest}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mathlib-root", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--limit", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=20260520)
    parser.add_argument("--source-repo", default="https://github.com/leanprover-community/mathlib4")
    parser.add_argument("--source-commit", default=None)
    parser.add_argument("--train-ratio", type=float, default=0.8)
    parser.add_argument("--val-ratio", type=float, default=0.1)
    parser.add_argument("--max-proof-chars", type=int, default=1200)
    args = parser.parse_args()
    result = generate_corpus(
        mathlib_root=args.mathlib_root,
        out_dir=args.out_dir,
        limit=args.limit,
        seed=args.seed,
        source_commit=args.source_commit,
        source_repo=args.source_repo,
        train_ratio=args.train_ratio,
        val_ratio=args.val_ratio,
        max_proof_chars=args.max_proof_chars,
    )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
