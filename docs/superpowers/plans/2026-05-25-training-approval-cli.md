# Training Approval CLI Plan

## Goal

Add a safe automation layer for training promotion approvals. The tool may
prepare and validate approval evidence, but it must not convert a run to
`human-reviewed` without an explicit operator action.

## Scope

- Add `refine training-approval draft` to validate a training agent report,
  policy, and evidence directory, then write `approvals/training.draft.json`
  plus `approvals/training.review-request.json`.
- Add `refine training-approval approve` to rerun the same validation and write
  `approvals/training.json` only when `--i-reviewed-this-evidence` and a named
  allowed human operator are provided.
- Add an approval policy example with required metrics, metric delta floors,
  required evidence paths, allowed operators, and smoke-run handling.
- Document that drafts are machine assistance only; the final approval file is
  the human trust boundary.

## Tests

- Draft command writes draft/request artifacts but never final approval.
- Approve command refuses to write final approval without the explicit review
  confirmation flag.
- Approve command writes final approval and resolves the review request with
  valid evidence.
- Policy regression thresholds block draft and approval.
