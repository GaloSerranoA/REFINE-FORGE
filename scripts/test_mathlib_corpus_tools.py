import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, rel: str):
    path = ROOT / rel
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec is not None and spec.loader is not None
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


class GenerateMathlibCorpusTests(unittest.TestCase):
    def test_generates_split_corpus_with_lsp_patch(self):
        mod = load_module(
            "generate_mathlib_repair_corpus",
            "scripts/generate_mathlib_repair_corpus.py",
        )
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            src = root / "Mathlib" / "Data" / "Nat" / "RefineforgeFixture.lean"
            src.parent.mkdir(parents=True)
            src.write_text(
                "\n".join(
                    [
                        "import Mathlib.Data.Nat.Basic",
                        "",
                        "theorem refineforge_add_zero (n : Nat) : n + 0 = n := by",
                        "  simpa using Nat.add_zero n",
                        "",
                        "instance : Inhabited Nat where",
                        "  default := 0",
                        "",
                        "lemma refineforge_zero_add (n : Nat) : 0 + n = n := by",
                        "  simpa using Nat.zero_add n",
                        "",
                        "/-- Next declaration docs must not be part of the lemma proof. -/",
                        "theorem refineforge_mul_one (n : Nat) : n * 1 = n := by",
                        "  simpa using Nat.mul_one n",
                        "",
                    ]
                ),
                encoding="utf-8",
            )
            out = root / "out"
            result = mod.generate_corpus(
                mathlib_root=root,
                out_dir=out,
                limit=3,
                seed=7,
                source_commit="abc123",
                source_repo="fixture",
                train_ratio=0.5,
                val_ratio=0.25,
            )

            self.assertEqual(result["written"], 3)
            all_rows = [
                json.loads(line)
                for line in (out / "all.jsonl").read_text(encoding="utf-8").splitlines()
            ]
            self.assertEqual(len(all_rows), 3)
            row = all_rows[0]
            self.assertIn("__refineforge_missing_proof__", row["broken_lean"])
            self.assertNotIn("__refineforge_missing_proof__", row["fixed_lean"])
            for generated in all_rows:
                self.assertNotIn("instance : Inhabited Nat", generated["fixed_lean"])
                self.assertNotIn("Next declaration docs", generated["fixed_lean"])
            self.assertEqual(row["source"]["commit"], "abc123")
            self.assertEqual(row["expected_patch"]["new_text"].splitlines()[0], "by")
            self.assertIsInstance(row["expected_patch"]["start_line"], int)
            self.assertEqual(
                sorted(p.name for p in out.glob("*.jsonl")),
                ["all.jsonl", "heldout.jsonl", "train.jsonl", "val.jsonl"],
            )
            manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["total_examples"], 3)
            self.assertEqual(manifest["splits"]["train"], 1)


class AnthropicTeacherToolTests(unittest.TestCase):
    def test_parses_json_response_and_builds_training_row(self):
        mod = load_module(
            "anthropic_teacher_generate",
            "scripts/anthropic_teacher_generate.py",
        )
        entry = {
            "id": "mathlib4-abc-000001",
            "split": "train",
            "prompt": "Diagnostic: unknown identifier\nSource:\n...",
            "expected_patch": {
                "start_line": 2,
                "start_char": 58,
                "end_line": 3,
                "end_char": 36,
                "new_text": "by\n  simpa using Nat.add_zero n",
            },
            "source": {"path": "Mathlib/Data/Nat/Fixture.lean"},
        }
        teacher_json = json.dumps(
            {
                "start_line": 2,
                "start_char": 58,
                "end_line": 3,
                "end_char": 36,
                "new_text": "by\n  simpa using Nat.add_zero n",
                "rationale": "Restore the original Mathlib proof.",
                "implied_theorem": "Adding zero to n returns n.",
            }
        )
        parsed = mod.parse_teacher_json(f"```json\n{teacher_json}\n```")
        self.assertEqual(parsed["new_text"], entry["expected_patch"]["new_text"])
        prompt = mod.build_prompt(entry)
        self.assertIn("byte-for-byte", prompt)
        self.assertIn("do not change tactic names", prompt)
        self.assertIn("push Not", prompt)
        row = mod.build_sft_row(
            entry,
            parsed,
            raw_text=teacher_json,
            model="claude-test",
            usage={"input_tokens": 10, "output_tokens": 20},
            cost_usd=0.001,
            stop_reason="end_turn",
        )
        self.assertEqual(row["id"], entry["id"])
        self.assertEqual(json.loads(row["response"]), json.loads(teacher_json))
        self.assertTrue(row["valid_response"])
        self.assertEqual(row["metadata"]["teacher"]["model"], "claude-test")

    def test_finalize_writes_splits_manifest_and_normalizes_invalid_patch(self):
        mod = load_module(
            "anthropic_teacher_generate",
            "scripts/anthropic_teacher_generate.py",
        )
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            source = root / "all.jsonl"
            output = root / "anthropic-sft.jsonl"
            entries = [
                {
                    "id": "mathlib4-fixture-000001",
                    "split": "train",
                    "prompt": "repair A",
                    "expected_patch": {
                        "start_line": 1,
                        "start_char": 2,
                        "end_line": 3,
                        "end_char": 4,
                        "new_text": "by\n  exact h",
                    },
                },
                {
                    "id": "mathlib4-fixture-000002",
                    "split": "heldout",
                    "prompt": "repair B",
                    "expected_patch": {
                        "start_line": 5,
                        "start_char": 6,
                        "end_line": 7,
                        "end_char": 8,
                        "new_text": "by\n  simp",
                    },
                },
            ]
            source.write_text(
                "".join(json.dumps(row) + "\n" for row in entries),
                encoding="utf-8",
            )
            good_response = {
                **entries[0]["expected_patch"],
                "rationale": "restore proof",
                "implied_theorem": "A",
            }
            bad_response = {
                **entries[1]["expected_patch"],
                "new_text": "by\n  omega",
                "rationale": "wrong patch",
                "implied_theorem": "B",
            }
            rows = [
                {
                    "id": entries[0]["id"],
                    "split": "train",
                    "prompt": "repair A",
                    "response": json.dumps(good_response),
                    "valid_response": True,
                    "metadata": {
                        "expected_patch": entries[0]["expected_patch"],
                        "teacher": {
                            "model": "claude-test",
                            "cost_usd_estimate": 0.01,
                        },
                    },
                },
                {
                    "id": entries[1]["id"],
                    "split": "heldout",
                    "prompt": "repair B",
                    "response": json.dumps(bad_response),
                    "valid_response": False,
                    "metadata": {
                        "expected_patch": entries[1]["expected_patch"],
                        "teacher": {
                            "model": "claude-test",
                            "cost_usd_estimate": 0.02,
                        },
                    },
                },
            ]
            output.write_text(
                "".join(json.dumps(row) + "\n" for row in reversed(rows)),
                encoding="utf-8",
            )

            manifest = mod.finalize_output(
                input_path=source,
                output_path=output,
                normalize_invalid_patches=True,
            )

            self.assertTrue(manifest["complete"])
            self.assertEqual(manifest["rows"], 2)
            self.assertEqual(manifest["splits"], {"train": 1, "val": 0, "heldout": 1})
            self.assertEqual(manifest["invalid_patch_rows"], 0)
            self.assertEqual(manifest["normalized_patch_rows"], 1)
            self.assertTrue((root / "anthropic-sft.train.jsonl").exists())
            self.assertTrue((root / "anthropic-sft.val.jsonl").exists())
            self.assertTrue((root / "anthropic-sft.heldout.jsonl").exists())
            rewritten = [
                json.loads(line)
                for line in output.read_text(encoding="utf-8").splitlines()
            ]
            self.assertEqual([row["id"] for row in rewritten], [e["id"] for e in entries])
            self.assertTrue(all(row["valid_response"] for row in rewritten))
            corrected = json.loads(rewritten[1]["response"])
            self.assertEqual(corrected["new_text"], entries[1]["expected_patch"]["new_text"])
            self.assertTrue(
                rewritten[1]["metadata"]["teacher"]["normalized_invalid_patch"]
            )

    def test_retry_invalid_refreshes_only_fallback_rows(self):
        mod = load_module(
            "anthropic_teacher_generate",
            "scripts/anthropic_teacher_generate.py",
        )
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            source = root / "all.jsonl"
            output = root / "anthropic-sft.jsonl"
            expected_patch = {
                "start_line": 1,
                "start_char": 2,
                "end_line": 3,
                "end_char": 4,
                "new_text": "by\n  simp",
            }
            entry = {
                "id": "mathlib4-fixture-000001",
                "split": "train",
                "prompt": "repair",
                "expected_patch": expected_patch,
            }
            source.write_text(json.dumps(entry) + "\n", encoding="utf-8")
            fallback = {
                "id": entry["id"],
                "split": "train",
                "prompt": "repair",
                "response": json.dumps(
                    {
                        **expected_patch,
                        "rationale": (
                            "Teacher response was invalid; deterministic expected "
                            "patch recorded for audit."
                        ),
                        "implied_theorem": "",
                    }
                ),
                "valid_response": True,
                "metadata": {
                    "expected_patch": expected_patch,
                    "teacher": {
                        "model": "claude-test",
                        "cost_usd_estimate": 0.01,
                    },
                },
            }
            output.write_text(json.dumps(fallback) + "\n", encoding="utf-8")

            calls = []
            old_generate_one = mod._generate_one
            old_key = os.environ.get("ANTHROPIC_API_KEY")

            def fake_generate_one(entry_arg, **_kwargs):
                calls.append(entry_arg["id"])
                parsed = {
                    **entry_arg["expected_patch"],
                    "rationale": "fresh teacher response",
                    "implied_theorem": "fixture",
                }
                return (
                    mod.build_sft_row(
                        entry_arg,
                        parsed,
                        raw_text=json.dumps(parsed),
                        model="claude-test",
                        usage={"input_tokens": 1, "output_tokens": 1},
                        cost_usd=0.001,
                        stop_reason="end_turn",
                    ),
                    0.001,
                    False,
                )

            try:
                os.environ["ANTHROPIC_API_KEY"] = "test-key"
                mod._generate_one = fake_generate_one
                result = mod.generate(
                    input_path=source,
                    output_path=output,
                    model="claude-test",
                    limit=1,
                    max_cost_usd=10.0,
                    max_tokens=128,
                    temperature=0.0,
                    sleep_seconds=0.0,
                    retries=1,
                    backoff_seconds=0.0,
                    concurrency=1,
                    retry_invalid=True,
                )
            finally:
                mod._generate_one = old_generate_one
                if old_key is None:
                    os.environ.pop("ANTHROPIC_API_KEY", None)
                else:
                    os.environ["ANTHROPIC_API_KEY"] = old_key

            self.assertEqual(calls, [entry["id"]])
            self.assertEqual(result["retried_invalid_rows"], 1)
            refreshed = json.loads(output.read_text(encoding="utf-8").strip())
            self.assertNotIn("Teacher response was invalid", refreshed["response"])
            self.assertTrue(result["finalized"]["complete"])


if __name__ == "__main__":
    unittest.main()
