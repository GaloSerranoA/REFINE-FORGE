#!/usr/bin/env bash
# refineforge release script (POSIX).
#
# Usage:
#   release/release.sh <version>      # e.g. release/release.sh 0.2.0
#   release/release.sh <version> --dry-run
#
# What it does:
#   1. Validates <version> is semver (X.Y.Z).
#   2. Confirms working tree is clean (no uncommitted changes).
#   3. Confirms we're on `main`.
#   4. Confirms a tag `v<version>` does not already exist locally or
#      on origin (if origin is configured).
#   5. Confirms CHANGELOG.md contains a section for the new version,
#      OR moves [Unreleased] entries into a new [<version>] section.
#   6. Bumps the workspace-package `version` field in Cargo.toml.
#   7. Runs `cargo check --workspace` to catch obvious breakage.
#   8. Runs `cargo nextest run --workspace`. STOP on any failure.
#   9. Runs `refine lean check-all`. STOP on any failure.
#  10. Stages the version bump + CHANGELOG, commits with the message
#      `release: v<version>`.
#  11. Creates an annotated tag `v<version>`. If `cosign` is on PATH,
#      ALSO produces a detached signature `release/v<version>.sig`
#      via `cosign sign-blob` on the tag's commit SHA.
#  12. Prints push instructions; does NOT push automatically.
#
# Owned by Section 3 (DevOps). See ARCHITECTURE.md.

set -euo pipefail

DRY_RUN=0
VERSION=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h|--help)
      sed -n '2,/^$/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      if [ -z "$VERSION" ]; then
        VERSION="$arg"
      else
        echo "release.sh: unexpected argument '$arg'" >&2
        exit 2
      fi
      ;;
  esac
done

if [ -z "$VERSION" ]; then
  echo "release.sh: missing <version> argument. Try: release/release.sh 0.2.0" >&2
  exit 2
fi

# 1. Semver check.
if ! echo "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$'; then
  echo "release.sh: version '$VERSION' is not semver (X.Y.Z or X.Y.Z-prerelease)" >&2
  exit 2
fi
TAG="v$VERSION"

# Repo root = script's parent dir.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

step() { echo "──── $* ────"; }
run()  { if [ "$DRY_RUN" -eq 1 ]; then echo "DRY-RUN: $*"; else eval "$@"; fi; }

# 2. Clean tree.
step "2. checking working tree is clean"
if [ -n "$(git status --porcelain)" ]; then
  echo "release.sh: working tree is dirty; commit or stash first" >&2
  git status --short
  exit 1
fi

# 3. On main.
step "3. checking we're on main"
BRANCH="$(git symbolic-ref --short HEAD)"
if [ "$BRANCH" != "main" ] && [ "$BRANCH" != "master" ]; then
  echo "release.sh: refusing to release from branch '$BRANCH' — checkout main first" >&2
  exit 1
fi

# 4. Tag does not exist.
step "4. checking tag $TAG does not exist"
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  echo "release.sh: tag $TAG already exists locally" >&2
  exit 1
fi
if git remote get-url origin >/dev/null 2>&1; then
  if git ls-remote --tags origin "refs/tags/$TAG" | grep -q "$TAG"; then
    echo "release.sh: tag $TAG already exists on origin" >&2
    exit 1
  fi
fi

# 5. CHANGELOG entry.
step "5. checking CHANGELOG has entry for $VERSION (or moving [Unreleased])"
if grep -q "^## \[$VERSION\]" CHANGELOG.md; then
  echo "CHANGELOG already has a [$VERSION] section. Good."
elif grep -q "^## \[Unreleased\]" CHANGELOG.md; then
  echo "Moving [Unreleased] entries into [$VERSION] section."
  TODAY="$(date -u +%Y-%m-%d)"
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "DRY-RUN: would rewrite [Unreleased] → [$VERSION] — $TODAY"
  else
    # Insert a fresh empty [Unreleased] above the existing one,
    # rename the old [Unreleased] to [$VERSION] — $TODAY.
    sed -i.bak \
      -e "0,/^## \[Unreleased\]/{s|^## \[Unreleased\]|## [Unreleased]\n\n(nothing yet)\n\n## [$VERSION] — $TODAY|}" \
      CHANGELOG.md
    rm CHANGELOG.md.bak
  fi
else
  echo "release.sh: CHANGELOG.md has neither [$VERSION] nor [Unreleased] — refusing" >&2
  exit 1
fi

# 6. Bump Cargo.toml workspace.package.version.
step "6. bumping workspace version to $VERSION"
if [ "$DRY_RUN" -eq 1 ]; then
  echo "DRY-RUN: would set [workspace.package].version = \"$VERSION\""
else
  python3 - "$VERSION" <<'PY'
import re, sys
v = sys.argv[1]
with open('Cargo.toml') as f:
    s = f.read()
# Replace within [workspace.package]
def sub(m):
    block = m.group(0)
    return re.sub(r'^version\s*=\s*"[^"]+"', f'version = "{v}"', block, count=1, flags=re.MULTILINE)
new = re.sub(r'\[workspace\.package\][^\[]*', sub, s, count=1)
if new == s:
    print("WARN: didn't find [workspace.package].version to bump", file=sys.stderr)
    sys.exit(1)
with open('Cargo.toml', 'w') as f:
    f.write(new)
print(f"bumped version -> {v}")
PY
fi

# 7. cargo check.
step "7. cargo check --workspace"
run "cargo check --workspace"

# 8. cargo nextest.
step "8. cargo nextest run --workspace (release profile to match CI)"
run "cargo nextest run --workspace"

# 9. lean check-all.
step "9. refine lean check-all"
# Discover the cargo target directory (respects CARGO_TARGET_DIR
# env override + workspace-shared target dirs) instead of
# assuming ./target/release/.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 2>/dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' \
  2>/dev/null || echo ./target)"
BIN_EXT=""
case "$(uname -s)" in
  CYGWIN*|MINGW*|MSYS*|Windows_NT) BIN_EXT=".exe" ;;
esac
DEFAULT_REFINE_BIN="$TARGET_DIR/release/refine${BIN_EXT}"
if ! command -v refine >/dev/null 2>&1 && [ ! -x "$DEFAULT_REFINE_BIN" ] && [ -z "${REFINE_BIN:-}" ]; then
  run "cargo build --release --bin refine"
fi
REFINE_BIN="${REFINE_BIN:-$DEFAULT_REFINE_BIN}"
run "$REFINE_BIN lean check-all"

# 10. Commit.
step "10. committing version bump"
run "git add Cargo.toml CHANGELOG.md"
run "git commit -m 'release: $TAG'"

# 11. Tag (+ optional cosign sign-blob of the tag commit SHA).
step "11. creating annotated tag $TAG"
run "git tag -a $TAG -m 'refineforge $TAG'"
if command -v cosign >/dev/null 2>&1; then
  COMMIT="$(git rev-parse "$TAG^{commit}")"
  SIG_PATH="release/$TAG.sig"
  echo "cosign is available — signing tag commit SHA"
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "DRY-RUN: cosign sign-blob --yes --output-signature $SIG_PATH (input: $COMMIT)"
  else
    printf '%s' "$COMMIT" | cosign sign-blob --yes \
      --output-signature "$SIG_PATH" \
      --output-certificate "release/$TAG.cert" \
      -
    echo "wrote $SIG_PATH (signature over commit SHA)"
  fi
else
  echo "(cosign not on PATH — skipping tag-commit signing; CI will sign bundles)"
fi

# 12. Push instructions.
echo
step "DONE"
echo "Local commit + tag are ready. To publish:"
echo "    git push origin main"
echo "    git push origin $TAG"
echo
echo "CI will:"
echo "    1. Build + verify on Ubuntu/macOS/Windows."
echo "    2. Sign every bundle's manifest.json with cosign (keyless)."
echo "    3. Upload signed bundles as workflow artifacts."
echo
echo "A reviewer can then verify a bundle with:"
echo "    refine bundle verify <bundle> --verify-signature"
