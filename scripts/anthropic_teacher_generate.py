"""Run resumable Anthropic teacher generation for proof-repair corpus rows."""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import os
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any


ANTHROPIC_URL = "https://api.anthropic.com/v1/messages"
DEFAULT_MODEL = "claude-sonnet-4-6"
PRICING_USD_PER_M_TOKENS = {
    "claude-opus-4-7": {"input": 15.00, "output": 75.00},
    "claude-sonnet-4-6": {"input": 3.00, "output": 15.00},
    "claude-haiku-4-5-20251001": {"input": 0.80, "output": 4.00},
}
DEFAULT_PRICING = {"input": 3.00, "output": 15.00}


def parse_teacher_json(text: str) -> dict[str, Any]:
    cleaned = text.strip()
    if cleaned.startswith("```"):
        lines = cleaned.splitlines()
        if lines and lines[0].startswith("```"):
            lines = lines[1:]
        if lines and lines[-1].startswith("```"):
            lines = lines[:-1]
        cleaned = "\n".join(lines).strip()
    data = json.loads(cleaned)
    if not isinstance(data, dict):
        raise ValueError("teacher response must be a JSON object")
    required = ["start_line", "start_char", "end_line", "end_char", "new_text"]
    missing = [k for k in required if k not in data]
    if missing:
        raise ValueError(f"teacher response missing keys: {missing}")
    return data


def estimate_cost(model: str, usage: dict[str, Any]) -> float:
    rates = PRICING_USD_PER_M_TOKENS.get(model, DEFAULT_PRICING)
    input_tokens = int(usage.get("input_tokens") or 0)
    output_tokens = int(usage.get("output_tokens") or 0)
    return (input_tokens * rates["input"] + output_tokens * rates["output"]) / 1_000_000.0


def build_prompt(entry: dict[str, Any]) -> str:
    expected = entry["expected_patch"]
    return (
        "You are generating supervised fine-tuning targets for Lean 4 proof repair.\n"
        "Return JSON only. Do not use markdown.\n\n"
        "The broken source was produced from a real Mathlib declaration by replacing "
        "the original proof with an unknown identifier. Emit the LSP-shaped patch "
        "that restores the proof, plus a concise rationale and implied_theorem.\n\n"
        "CRITICAL: copy the supplied start_line, start_char, end_line, end_char, "
        "and new_text byte-for-byte from the expected patch below. Do not simplify "
        "Lean code, do not change tactic names, do not add or remove leading "
        "newlines, and do not omit the leading `by` when it appears in new_text. "
        "If new_text contains `push Not`, keep exactly `push Not`; do not replace "
        "it with `push_neg`.\n\n"
        "Required JSON keys: start_line, start_char, end_line, end_char, new_text, "
        "rationale, implied_theorem.\n\n"
        "Use these exact range coordinates and exact new_text:\n"
        + json.dumps(expected, ensure_ascii=False, indent=2)
        + "\n\n"
        "Corpus prompt:\n"
        + str(entry["prompt"])
    )


def _request_anthropic(
    *,
    api_key: str,
    model: str,
    prompt: str,
    max_tokens: int,
    temperature: float,
) -> dict[str, Any]:
    payload = {
        "model": model,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "messages": [{"role": "user", "content": prompt}],
    }
    req = urllib.request.Request(
        ANTHROPIC_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        return json.loads(resp.read().decode("utf-8"))


def call_anthropic_with_retries(
    *,
    api_key: str,
    model: str,
    prompt: str,
    max_tokens: int,
    temperature: float,
    retries: int,
    backoff_seconds: float,
) -> dict[str, Any]:
    last_error: Exception | None = None
    for attempt in range(1, retries + 1):
        try:
            return _request_anthropic(
                api_key=api_key,
                model=model,
                prompt=prompt,
                max_tokens=max_tokens,
                temperature=temperature,
            )
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="replace")
            last_error = RuntimeError(f"HTTP {exc.code}: {body[:500]}")
            if exc.code not in (429, 500, 502, 503, 529) or attempt >= retries:
                raise last_error
        except Exception as exc:
            last_error = exc
            if attempt >= retries:
                raise
        time.sleep(backoff_seconds * attempt)
    raise RuntimeError(f"Anthropic retries exhausted: {last_error!r}")


def _content_text(data: dict[str, Any]) -> str:
    blocks = data.get("content") or []
    return "".join(
        str(block.get("text", ""))
        for block in blocks
        if isinstance(block, dict) and block.get("type") == "text"
    )


def build_sft_row(
    entry: dict[str, Any],
    parsed: dict[str, Any],
    *,
    raw_text: str,
    model: str,
    usage: dict[str, Any],
    cost_usd: float,
    stop_reason: str,
) -> dict[str, Any]:
    response = json.dumps(parsed, ensure_ascii=False, sort_keys=True)
    expected_new_text = entry["expected_patch"]["new_text"]
    valid = parsed.get("new_text") == expected_new_text
    return {
        "id": entry["id"],
        "split": entry.get("split", "train"),
        "prompt": entry["prompt"],
        "response": response,
        "valid_response": valid,
        "metadata": {
            "source": entry.get("source"),
            "mutation": entry.get("mutation"),
            "expected_patch": entry.get("expected_patch"),
            "teacher": {
                "provider": "anthropic",
                "model": model,
                "usage": usage,
                "cost_usd_estimate": round(cost_usd, 8),
                "stop_reason": stop_reason,
                "raw_response_sha256": hashlib.sha256(raw_text.encode("utf-8")).hexdigest(),
            },
        },
    }


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def _write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8", newline="\n") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")


def _read_existing_ids(path: Path) -> tuple[set[str], float]:
    if not path.exists():
        return set(), 0.0
    ids: set[str] = set()
    cost = 0.0
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        ids.add(str(row["id"]))
        cost += float(row.get("metadata", {}).get("teacher", {}).get("cost_usd_estimate") or 0.0)
    return ids, cost


def _teacher_cost(row: dict[str, Any]) -> float:
    return float(row.get("metadata", {}).get("teacher", {}).get("cost_usd_estimate") or 0.0)


def _manifest_path(output_path: Path) -> Path:
    return output_path.with_suffix(".manifest.json")


def _split_path(output_path: Path, split: str) -> Path:
    return output_path.with_name(f"{output_path.stem}.{split}{output_path.suffix}")


def _response_json(row: dict[str, Any]) -> dict[str, Any]:
    response = row.get("response", "{}")
    try:
        parsed = json.loads(response)
    except Exception:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def _expected_patch(row: dict[str, Any], input_by_id: dict[str, dict[str, Any]]) -> dict[str, Any]:
    metadata_patch = row.get("metadata", {}).get("expected_patch")
    if isinstance(metadata_patch, dict):
        return metadata_patch
    source_entry = input_by_id.get(str(row.get("id")))
    if source_entry is None:
        return {}
    patch = source_entry.get("expected_patch")
    return patch if isinstance(patch, dict) else {}


def _is_fallback_response(parsed: dict[str, Any]) -> bool:
    return str(parsed.get("rationale", "")).startswith("Teacher response was invalid")


def _needs_teacher_refresh(row: dict[str, Any], input_by_id: dict[str, dict[str, Any]]) -> bool:
    parsed = _response_json(row)
    expected = _expected_patch(row, input_by_id)
    teacher = row.get("metadata", {}).get("teacher", {})
    normalized = isinstance(teacher, dict) and bool(teacher.get("normalized_invalid_patch"))
    invalid_patch = parsed.get("new_text") != expected.get("new_text")
    return bool(
        normalized
        or invalid_patch
        or _is_fallback_response(parsed)
        or row.get("valid_response") is False
    )


def _prepare_existing_output(
    *,
    input_rows: list[dict[str, Any]],
    output_path: Path,
    retry_invalid: bool,
) -> tuple[set[str], float, int]:
    if not output_path.exists():
        return set(), 0.0, 0

    input_by_id = {str(row["id"]): row for row in input_rows}
    kept_rows: list[dict[str, Any]] = []
    refreshed = 0
    for row in _load_jsonl(output_path):
        row_id = str(row.get("id", ""))
        if retry_invalid and row_id in input_by_id and _needs_teacher_refresh(row, input_by_id):
            refreshed += 1
            continue
        kept_rows.append(row)

    if retry_invalid and refreshed:
        _write_jsonl(output_path, kept_rows)

    existing = {str(row["id"]) for row in kept_rows if "id" in row}
    spent = sum(_teacher_cost(row) for row in kept_rows)
    return existing, spent, refreshed


def _normalize_invalid_patch(
    row: dict[str, Any],
    *,
    expected_patch: dict[str, Any],
    parsed_response: dict[str, Any],
) -> bool:
    expected_new_text = expected_patch.get("new_text")
    if parsed_response.get("new_text") == expected_new_text:
        row["valid_response"] = True
        return False

    metadata = row.setdefault("metadata", {})
    if not isinstance(metadata, dict):
        metadata = {}
        row["metadata"] = metadata
    teacher = metadata.setdefault("teacher", {})
    if not isinstance(teacher, dict):
        teacher = {}
        metadata["teacher"] = teacher
    original_response = str(row.get("response", ""))
    teacher["normalized_invalid_patch"] = True
    teacher["invalid_patch_original_response_sha256"] = hashlib.sha256(
        original_response.encode("utf-8")
    ).hexdigest()
    teacher["invalid_patch_reason"] = "teacher new_text did not match expected_patch"

    corrected = {
        **expected_patch,
        "rationale": (
            "Teacher patch did not match expected_patch; deterministic expected "
            "Mathlib patch recorded for training and audit."
        ),
        "implied_theorem": str(parsed_response.get("implied_theorem", "")),
    }
    row["response"] = json.dumps(corrected, ensure_ascii=False, sort_keys=True)
    row["valid_response"] = True
    return True


def finalize_output(
    *,
    input_path: Path,
    output_path: Path,
    limit: int | None = None,
    normalize_invalid_patches: bool = False,
) -> dict[str, Any]:
    input_rows = _load_jsonl(input_path)
    if limit is not None:
        input_rows = input_rows[:limit]
    input_ids = [str(row["id"]) for row in input_rows]
    input_by_id = {str(row["id"]): row for row in input_rows}

    output_rows = _load_jsonl(output_path) if output_path.exists() else []
    by_id: dict[str, dict[str, Any]] = {}
    duplicate_lines = 0
    extra_ids: list[str] = []
    for row in output_rows:
        row_id = str(row.get("id", ""))
        if row_id not in input_by_id:
            extra_ids.append(row_id)
            continue
        if row_id in by_id:
            duplicate_lines += 1
        by_id[row_id] = row

    canonical = [by_id[row_id] for row_id in input_ids if row_id in by_id]
    missing_ids = [row_id for row_id in input_ids if row_id not in by_id]

    fallback_rows = 0
    invalid_patch_rows_before = 0
    normalized_patch_rows = 0
    valid_patch_rows = 0
    for row in canonical:
        parsed = _response_json(row)
        expected = _expected_patch(row, input_by_id)
        if _is_fallback_response(parsed):
            fallback_rows += 1
        if parsed.get("new_text") != expected.get("new_text"):
            invalid_patch_rows_before += 1
            if normalize_invalid_patches:
                normalized_patch_rows += int(
                    _normalize_invalid_patch(
                        row,
                        expected_patch=expected,
                        parsed_response=parsed,
                    )
                )
                parsed = _response_json(row)
        row["valid_response"] = parsed.get("new_text") == expected.get("new_text")
        valid_patch_rows += int(bool(row["valid_response"]))

    _write_jsonl(output_path, canonical)
    for split in ("train", "val", "heldout"):
        _write_jsonl(_split_path(output_path, split), [r for r in canonical if r.get("split") == split])

    split_counts = Counter(str(row.get("split", "")) for row in canonical)
    teacher_models = Counter(
        str(row.get("metadata", {}).get("teacher", {}).get("model", ""))
        for row in canonical
    )
    estimated_spend = sum(
        float(row.get("metadata", {}).get("teacher", {}).get("cost_usd_estimate") or 0.0)
        for row in canonical
    )
    manifest = {
        "complete": not missing_ids and len(canonical) == len(input_rows),
        "source": str(input_path),
        "output": str(output_path),
        "input_rows": len(input_rows),
        "rows": len(canonical),
        "unique_ids": len({str(row["id"]) for row in canonical}),
        "duplicate_output_lines": duplicate_lines,
        "extra_output_rows": len(extra_ids),
        "missing_rows": len(missing_ids),
        "valid_patch_rows": valid_patch_rows,
        "invalid_patch_rows": len(canonical) - valid_patch_rows,
        "invalid_patch_rows_before_normalization": invalid_patch_rows_before,
        "normalized_patch_rows": normalized_patch_rows,
        "fallback_teacher_responses": fallback_rows,
        "estimated_spend_usd": round(estimated_spend, 6),
        "splits": {split: split_counts.get(split, 0) for split in ("train", "val", "heldout")},
        "teacher_models": dict(sorted(teacher_models.items())),
        "files": {
            "all": output_path.name,
            "train": _split_path(output_path, "train").name,
            "val": _split_path(output_path, "val").name,
            "heldout": _split_path(output_path, "heldout").name,
            "manifest": _manifest_path(output_path).name,
        },
    }
    if missing_ids:
        manifest["missing_sample"] = missing_ids[:20]
    _manifest_path(output_path).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest


def _generate_one(
    entry: dict[str, Any],
    *,
    api_key: str,
    model: str,
    max_tokens: int,
    temperature: float,
    retries: int,
    backoff_seconds: float,
) -> tuple[dict[str, Any], float, bool]:
    prompt = build_prompt(entry)
    data = call_anthropic_with_retries(
        api_key=api_key,
        model=model,
        prompt=prompt,
        max_tokens=max_tokens,
        temperature=temperature,
        retries=retries,
        backoff_seconds=backoff_seconds,
    )
    raw_text = _content_text(data)
    usage = data.get("usage") or {}
    cost = estimate_cost(model, usage)
    invalid = False
    try:
        parsed = parse_teacher_json(raw_text)
    except Exception:
        invalid = True
        parsed = {
            **entry["expected_patch"],
            "rationale": "Teacher response was invalid; deterministic expected patch recorded for audit.",
            "implied_theorem": "",
        }
    row = build_sft_row(
        entry,
        parsed,
        raw_text=raw_text,
        model=model,
        usage=usage,
        cost_usd=cost,
        stop_reason=str(data.get("stop_reason") or ""),
    )
    return row, cost, invalid


def generate(
    *,
    input_path: Path,
    output_path: Path,
    model: str,
    limit: int | None,
    max_cost_usd: float,
    max_tokens: int,
    temperature: float,
    sleep_seconds: float,
    retries: int,
    backoff_seconds: float,
    concurrency: int = 1,
    retry_invalid: bool = False,
    normalize_invalid_patches: bool = False,
    dry_run: bool = False,
) -> dict[str, Any]:
    api_key = os.environ.get("ANTHROPIC_API_KEY", "")
    if not dry_run and not api_key:
        raise RuntimeError("ANTHROPIC_API_KEY is not set")
    rows = _load_jsonl(input_path)
    if limit is not None:
        rows = rows[:limit]
    existing, spent, retrying = _prepare_existing_output(
        input_rows=rows,
        output_path=output_path,
        retry_invalid=retry_invalid,
    )
    output_path.parent.mkdir(parents=True, exist_ok=True)
    written = 0
    skipped = 0
    failures = 0
    if dry_run:
        return {
            "input_rows": len(rows),
            "existing_rows": len(existing),
            "would_call": sum(1 for r in rows if str(r["id"]) not in existing),
            "model": model,
            "retrying_invalid_rows": retrying,
        }
    with output_path.open("a", encoding="utf-8", newline="\n") as out:
        pending = [entry for entry in rows if str(entry["id"]) not in existing]
        skipped = len(rows) - len(pending)
        if concurrency <= 1:
            for entry in pending:
                if spent >= max_cost_usd:
                    break
                row, cost, invalid = _generate_one(
                    entry,
                    api_key=api_key,
                    model=model,
                    max_tokens=max_tokens,
                    temperature=temperature,
                    retries=retries,
                    backoff_seconds=backoff_seconds,
                )
                spent += cost
                failures += int(invalid)
                out.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
                out.flush()
                written += 1
                if sleep_seconds:
                    time.sleep(sleep_seconds)
        else:
            if spent >= max_cost_usd:
                pending = []
            max_workers = max(1, concurrency)
            with ThreadPoolExecutor(max_workers=max_workers) as pool:
                futures = {
                    pool.submit(
                        _generate_one,
                        entry,
                        api_key=api_key,
                        model=model,
                        max_tokens=max_tokens,
                        temperature=temperature,
                        retries=retries,
                        backoff_seconds=backoff_seconds,
                    ): entry
                    for entry in pending
                }
                for future in as_completed(futures):
                    row, cost, invalid = future.result()
                    spent += cost
                    failures += int(invalid)
                    out.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
                    out.flush()
                    written += 1
                    if sleep_seconds:
                        time.sleep(sleep_seconds)
    result = {
        "input_rows": len(rows),
        "written": written,
        "skipped": skipped,
        "retried_invalid_rows": retrying,
        "invalid_teacher_responses": failures,
        "estimated_spend_usd": round(spent, 6),
        "output": str(output_path),
        "model": model,
    }
    result["finalized"] = finalize_output(
        input_path=input_path,
        output_path=output_path,
        limit=limit,
        normalize_invalid_patches=normalize_invalid_patches,
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", default=os.environ.get("ANTHROPIC_MODEL") or DEFAULT_MODEL)
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument("--max-cost-usd", type=float, default=100.0)
    parser.add_argument("--max-tokens", type=int, default=512)
    parser.add_argument("--temperature", type=float, default=0.0)
    parser.add_argument("--sleep-seconds", type=float, default=0.0)
    parser.add_argument("--retries", type=int, default=3)
    parser.add_argument("--backoff-seconds", type=float, default=1.0)
    parser.add_argument("--concurrency", type=int, default=1)
    parser.add_argument("--retry-invalid", action="store_true")
    parser.add_argument("--finalize-only", action="store_true")
    parser.add_argument("--normalize-invalid-patches", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.finalize_only:
        result = finalize_output(
            input_path=args.input,
            output_path=args.output,
            limit=args.limit,
            normalize_invalid_patches=args.normalize_invalid_patches,
        )
    else:
        result = generate(
            input_path=args.input,
            output_path=args.output,
            model=args.model,
            limit=args.limit,
            max_cost_usd=args.max_cost_usd,
            max_tokens=args.max_tokens,
            temperature=args.temperature,
            sleep_seconds=args.sleep_seconds,
            retries=args.retries,
            backoff_seconds=args.backoff_seconds,
            concurrency=args.concurrency,
            retry_invalid=args.retry_invalid,
            normalize_invalid_patches=args.normalize_invalid_patches,
            dry_run=args.dry_run,
        )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
