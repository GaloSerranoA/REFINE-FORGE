# Stub trainer (PowerShell). Mirror of stub-trainer.sh.
param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$OutputDir,
    [int]$Steps = 10,
    [int]$FailAt = 0
)

$ErrorActionPreference = 'Stop'

New-Item -ItemType Directory -Force -Path (Join-Path $OutputDir 'checkpoints') | Out-Null

for ($i = 1; $i -le $Steps; $i++) {
    $loss = [math]::Round(1.0 / (1 + $i), 4)
    $lr   = [math]::Round(0.0002 * (1 - $i / $Steps), 6)
    $epoch = [math]::Round($i / 10.0, 4)
    Write-Output "{'loss': $loss, 'learning_rate': $lr, 'epoch': $epoch, 'step': $i}"

    if ($i % 5 -eq 0) {
        $ckpt = Join-Path $OutputDir "checkpoints\step-$i"
        New-Item -ItemType Directory -Force -Path $ckpt | Out-Null
        Set-Content -Path (Join-Path $ckpt 'model.bin') -Value "fake-weights-at-step-$i" -NoNewline
    }

    if ($FailAt -gt 0 -and $i -eq $FailAt) {
        Write-Error "RuntimeError: CUDA out of memory (simulated)"
        exit 1
    }
}

Write-Output "training completed normally"
exit 0
