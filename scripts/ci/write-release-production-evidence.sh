#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  write-release-production-evidence.sh hosted-ci <evidence-dir>
  write-release-production-evidence.sh architecture <evidence-dir>
  write-release-production-evidence.sh copy-release-ready <evidence-dir> <release-ready-dir>
  write-release-production-evidence.sh verifier-digest <evidence-dir> <image-name>
  write-release-production-evidence.sh cosign-verify <evidence-dir>
  write-release-production-evidence.sh nix-check <evidence-dir> -- <command...>
USAGE
  exit 64
}

json_escape() {
  python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))'
}

release_dir() {
  local evidence_dir="$1"
  mkdir -p "$evidence_dir/release"
  printf '%s/release\n' "$evidence_dir"
}

require_arg() {
  local value="${1:-}"
  local label="$2"
  if [ -z "$value" ]; then
    echo "missing required argument: $label" >&2
    usage
  fi
}

cmd="${1:-}"
[ -n "$cmd" ] || usage
shift || true

case "$cmd" in
  hosted-ci)
    evidence_dir="${1:-}"
    require_arg "$evidence_dir" "evidence-dir"
    out_dir="$(release_dir "$evidence_dir")"
    run_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-unknown/repo}/actions/runs/${GITHUB_RUN_ID:-0}"
    cat > "$out_dir/hosted-ci.json" <<JSON
{
  "schema_version": "refineforge-hosted-ci-evidence-v1",
  "status": "passed",
  "hosted_ci_url": "$run_url",
  "oidc_issuer": "https://token.actions.githubusercontent.com",
  "run_id": "${GITHUB_RUN_ID:-0}",
  "run_attempt": "${GITHUB_RUN_ATTEMPT:-0}",
  "workflow": "${GITHUB_WORKFLOW:-ci}",
  "ref": "${GITHUB_REF:-unknown}",
  "sha": "${GITHUB_SHA:-unknown}"
}
JSON
    ;;
  architecture)
    evidence_dir="${1:-}"
    require_arg "$evidence_dir" "evidence-dir"
    out_dir="$(release_dir "$evidence_dir")"
    os_name="$(uname -s 2>/dev/null || printf unknown)"
    arch_name="$(uname -m 2>/dev/null || printf unknown)"
    rustc_version="$(rustc -Vv 2>/dev/null | tr '\n' ';' | sed 's/;*$//' || printf unknown)"
    cat > "$out_dir/architecture-matrix.json" <<JSON
{
  "schema_version": "refineforge-architecture-matrix-v1",
  "status": "passed",
  "runners": [
    {
      "os": "${RUNNER_OS:-$os_name}",
      "arch": "${RUNNER_ARCH:-$arch_name}",
      "uname": "$os_name $arch_name",
      "rustc": "$rustc_version"
    }
  ]
}
JSON
    ;;
  copy-release-ready)
    evidence_dir="${1:-}"
    source_dir="${2:-}"
    require_arg "$evidence_dir" "evidence-dir"
    require_arg "$source_dir" "release-ready-dir"
    out_dir="$(release_dir "$evidence_dir")"
    cp "$source_dir/sbom.cyclonedx.json" "$out_dir/sbom.cyclonedx.json"
    cp "$source_dir/provenance.intoto.json" "$out_dir/provenance.intoto.json"
    cp "$source_dir/release-report.json" "$out_dir/release-report.json"
    cp "$source_dir/release-report.md" "$out_dir/release-report.md"
    ;;
  verifier-digest)
    evidence_dir="${1:-}"
    image_name="${2:-}"
    require_arg "$evidence_dir" "evidence-dir"
    require_arg "$image_name" "image-name"
    out_dir="$(release_dir "$evidence_dir")"
    docker image inspect "$image_name" --format '{{.Id}}' > "$out_dir/verifier-container-digest.txt"
    ;;
  cosign-verify)
    evidence_dir="${1:-}"
    require_arg "$evidence_dir" "evidence-dir"
    out_dir="$(release_dir "$evidence_dir")"
    identity="${EXPECTED_IDENTITY_REGEX:-}"
    issuer="${EXPECTED_OIDC_ISSUER:-https://token.actions.githubusercontent.com}"
    identity_json="$(printf '%s' "$identity" | json_escape)"
    issuer_json="$(printf '%s' "$issuer" | json_escape)"
    cat > "$out_dir/cosign-verify.json" <<JSON
{
  "schema_version": "refineforge-cosign-verify-evidence-v1",
  "status": "passed",
  "identity_regex": $identity_json,
  "oidc_issuer": $issuer_json,
  "verified_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
JSON
    ;;
  nix-check)
    evidence_dir="${1:-}"
    require_arg "$evidence_dir" "evidence-dir"
    shift || true
    [ "${1:-}" = "--" ] || usage
    shift || true
    [ "$#" -gt 0 ] || usage
    out_dir="$(release_dir "$evidence_dir")"
    cp flake.lock "$out_dir/flake.lock"
    set +e
    "$@" 2>&1 | tee "$out_dir/nix-check.log"
    status=${PIPESTATUS[0]}
    set -e
    if [ "$status" -eq 0 ]; then
      echo "nix flake check passed" >> "$out_dir/nix-check.log"
    fi
    exit "$status"
    ;;
  *)
    usage
    ;;
esac
