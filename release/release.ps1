# refineforge release script (PowerShell mirror of release.sh).
#
# Usage:
#   .\release\release.ps1 -Version 0.2.0
#   .\release\release.ps1 -Version 0.2.0 -DryRun
#
# See release/release.sh for full step-by-step. This script is the
# Windows-friendly variant; behaviour is identical.

param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$Version,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

function Step($msg) { Write-Host "──── $msg ────" -ForegroundColor Cyan }
function Run($cmd) {
    if ($DryRun) { Write-Host "DRY-RUN: $cmd" -ForegroundColor Yellow }
    else { Invoke-Expression $cmd; if ($LASTEXITCODE -ne 0) { throw "command failed: $cmd" } }
}

# 1. Semver check.
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.-]+)?$') {
    throw "version '$Version' is not semver (X.Y.Z or X.Y.Z-prerelease)"
}
$Tag = "v$Version"

# Repo root = script's parent dir.
$Root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $Root

# 2. Clean tree.
Step "2. checking working tree is clean"
$status = git status --porcelain
if ($status) { git status --short; throw "working tree is dirty; commit or stash first" }

# 3. On main.
Step "3. checking we're on main"
$branch = git symbolic-ref --short HEAD
if ($branch -ne 'main' -and $branch -ne 'master') {
    throw "refusing to release from branch '$branch' — checkout main first"
}

# 4. Tag does not exist.
Step "4. checking tag $Tag does not exist"
git rev-parse -q --verify "refs/tags/$Tag" 2>$null
if ($LASTEXITCODE -eq 0) { throw "tag $Tag already exists locally" }
$origin = git remote get-url origin 2>$null
if ($LASTEXITCODE -eq 0) {
    $remoteTag = git ls-remote --tags origin "refs/tags/$Tag" | Select-String $Tag
    if ($remoteTag) { throw "tag $Tag already exists on origin" }
}

# 5. CHANGELOG entry.
Step "5. checking CHANGELOG has entry for $Version (or moving [Unreleased])"
$changelog = Get-Content CHANGELOG.md -Raw
if ($changelog -match "## \[$Version\]") {
    Write-Host "CHANGELOG already has a [$Version] section. Good."
} elseif ($changelog -match '## \[Unreleased\]') {
    Write-Host "Moving [Unreleased] entries into [$Version] section."
    $today = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
    if ($DryRun) {
        Write-Host "DRY-RUN: would rewrite [Unreleased] → [$Version] — $today"
    } else {
        $new = $changelog -replace '## \[Unreleased\]', "## [Unreleased]`n`n(nothing yet)`n`n## [$Version] — $today"
        # Only rewrite the FIRST occurrence.
        $first = $changelog.IndexOf('## [Unreleased]')
        $rewritten = $changelog.Substring(0,$first) + "## [Unreleased]`n`n(nothing yet)`n`n## [$Version] — $today" + $changelog.Substring($first + '## [Unreleased]'.Length)
        Set-Content -Path CHANGELOG.md -Value $rewritten -NoNewline
    }
} else {
    throw "CHANGELOG.md has neither [$Version] nor [Unreleased] — refusing"
}

# 6. Bump Cargo.toml workspace.package.version.
Step "6. bumping workspace version to $Version"
if (-not $DryRun) {
    $cargo = Get-Content Cargo.toml -Raw
    $pattern = '(?ms)(\[workspace\.package\][^\[]*?version\s*=\s*")[^"]+(")'
    if ($cargo -notmatch $pattern) { throw "couldn't find [workspace.package].version" }
    $new = [regex]::Replace($cargo, $pattern, "`${1}$Version`${2}", 1)
    Set-Content -Path Cargo.toml -Value $new -NoNewline
    Write-Host "bumped version → $Version"
} else {
    Write-Host "DRY-RUN: would set [workspace.package].version = `"$Version`""
}

# 7-9. Build + test + lean check.
Step "7. cargo check --workspace"
Run "cargo check --workspace"

Step "8. cargo nextest run --workspace"
Run "cargo nextest run --workspace"

Step "9. refine lean check-all"
$refine = $env:REFINE_BIN
if (-not $refine) {
    if (Test-Path .\target\release\refine.exe) { $refine = '.\target\release\refine.exe' }
    else { Run "cargo build --release --bin refine"; $refine = '.\target\release\refine.exe' }
}
Run "$refine lean check-all"

# 10. Commit.
Step "10. committing version bump"
Run "git add Cargo.toml CHANGELOG.md"
Run "git commit -m 'release: $Tag'"

# 11. Tag + optional cosign.
Step "11. creating annotated tag $Tag"
Run "git tag -a $Tag -m 'refineforge $Tag'"
$cosignAvailable = Get-Command cosign -ErrorAction SilentlyContinue
if ($cosignAvailable) {
    $commit = git rev-parse "$Tag^{commit}"
    $sigPath = "release\$Tag.sig"
    Write-Host "cosign is available — signing tag commit SHA"
    if ($DryRun) {
        Write-Host "DRY-RUN: cosign sign-blob --yes --output-signature $sigPath (input: $commit)"
    } else {
        $tmp = New-TemporaryFile
        Set-Content -Path $tmp -Value $commit -NoNewline
        cosign sign-blob --yes --output-signature $sigPath --output-certificate "release\$Tag.cert" $tmp
        Remove-Item $tmp
        Write-Host "wrote $sigPath (signature over commit SHA)"
    }
} else {
    Write-Host "(cosign not on PATH — skipping tag-commit signing; CI will sign bundles)"
}

# 12. Push instructions.
Write-Host ""
Step "DONE"
Write-Host "Local commit + tag are ready. To publish:"
Write-Host "    git push origin main"
Write-Host "    git push origin $Tag"
Write-Host ""
Write-Host "CI will sign bundles after the tag push (.github/workflows/ci.yml)."
Write-Host "A reviewer verifies a bundle with:"
Write-Host "    refine bundle verify <bundle> --verify-signature"
