# Stub: emits DIFFERENT bytes on every invocation. PowerShell mirror.
$r = Get-Random
Write-Host "nondeterministic-output-$r-$PID"
exit 0
