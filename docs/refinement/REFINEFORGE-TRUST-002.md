# REFINEFORGE-TRUST-002 — Refinement argument: operator-identity gate

> **Status: model+refined (human-reviewed 2026-05-29 by Galo Serrano Abad).**
> Dogfood claim: Lean proves a property of a model of Refine-Forge's *own*
> anti-spoofing gate (`is_automated_operator`), the check that keeps
> `review.human_operator` honest by rejecting AI/bot/placeholder names. The §6
> `[needs human]` items were reviewed and confirmed — **including the denylist
> limitation** — so `review.human_operator` is populated and the Lean agent
> reports `human-reviewed`. (Changing the Rust `blocked` array or the
> tokenisation invalidates this review and requires re-certification.)

## 1. What the Lean model says

In `lean/Refineforge/OperatorGate.lean` (`Refineforge.OperatorGate`):

| Lean entity | Kind | Meaning |
|---|---|---|
| `blocked` | def | the 15-token blocklist (`ai`, `bot`, `claude`, `gpt`, `null`, `placeholder`, …) |
| `isAutomated` | function | `tokens.any (· ∈ blocked)` — true iff any token is blocked |
| `blocked_token_is_rejected` | theorem | **T1:** a blocked token among the tokens ⇒ `isAutomated = true` |
| `clean_operator_is_accepted` | theorem | **T2:** no blocked token ⇒ `isAutomated = false` |
| `ai_name_is_rejected` | theorem | **T3:** `isAutomated ["claude"] = true` (concrete anti-spoofing) |

T1 is the safety direction (known automated names are rejected); T2 shows the
gate is not vacuously "reject everyone"; T3 is the concrete case that matters —
an AI cannot record itself as a human approver.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `fn is_automated_operator` | function | `crates/refineforge-cli/src/agent/common.rs` | `isAutomated` (+ `blocked`) |

`refine scan check REFINEFORGE-TRUST-002` confirms the symbol exists at the path.

## 3. Mapping

### 3.1 `blocked` ↔ the Rust `blocked` array

Rust `is_automated_operator` defines
`let blocked = ["ai","automated","automation","bot","chatgpt","claude","codex","gemini","gpt","llm","none","null","placeholder","tbd","todo"];`
The Lean `blocked` lists the same 15 tokens. A reviewer must confirm the two
lists are identical (a token added to or removed from the Rust array without
updating Lean would break the correspondence).

### 3.2 `isAutomated` ↔ the Rust membership check

Rust:

```rust
pub fn is_automated_operator(operator: &str) -> bool {
    let lower = operator.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let blocked = [/* 15 tokens */];
    tokens.iter().any(|t| blocked.contains(t))
}
```

The Lean `isAutomated tokens = tokens.any (· ∈ blocked)` mirrors the final
`tokens.iter().any(...)`. **Idealisation (out of model scope):** the Rust
lowercasing + `split(non-alphanumeric)` + `filter(non-empty)` that *produces*
the tokens is NOT modelled — the Lean takes the token list as given. So the Lean
proves: *given faithful tokenisation*, a blocked token is rejected. A reviewer
must accept that the tokenisation feeds the membership check the tokens the
model assumes.

## 4. Trusted code base

Conditional on: (1) Lean's kernel; (2) the Lean compiler v4.29.1; (3)
`rustc`/LLVM; (4) the Rust standard library (`str::to_ascii_lowercase`,
`str::split`, slice `contains`); and (5) the §3.1/§3.2 correspondences. We make
**no** claim that any of (1)–(4) is itself verified.

## 5. What this claim does NOT cover

- **Completeness of the denylist.** This is a *denylist*: it rejects the 15
  known automated/placeholder tokens. It does **not** prove that every possible
  AI/automated identity is caught — a novel name not on the list passes. The
  gate raises the bar against casual spoofing; it is not a complete AI detector.
- The tokenisation itself (lowercasing, splitting, Unicode handling). E.g.
  whether `"cl_aude"` tokenises to include `"claude"` is the tokeniser's job,
  not modelled here.
- Non-ASCII case folding (`to_ascii_lowercase` only lowercases ASCII).
- The surrounding approval flow (`validate_human_approval`) that *calls* this
  gate — covered, if at all, by a separate claim.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-002` →
      `Verified sorries=0 admits=0 axioms=0`.
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-002` →
      `Verified` (`is_automated_operator` present).
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-002`
      succeeds.
- [x] **[needs human]** The Lean `blocked` list equals the Rust `blocked` array
      (same 15 tokens). *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** The §3.2 tokenisation idealisation is acceptable — the
      Rust feeds the membership check the tokens the model assumes. *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** The denylist limitation in §5 is acceptable for how
      `human_operator` is used (a denylist, not a complete AI detector, is the
      intended guarantee). *(Galo Serrano Abad, 2026-05-29.)*

Reviewed and confirmed by **Galo Serrano Abad on 2026-05-29**;
`review.human_operator` is populated and the claim is `human-reviewed` / scope
`model+refined`. Changing the Rust `blocked` array, the tokenisation, or how
`human_operator` is consumed invalidates this review and requires
re-certification.
