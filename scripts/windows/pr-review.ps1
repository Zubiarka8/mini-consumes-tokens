$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot '../pr-review.py'
if (Get-Command python -ErrorAction SilentlyContinue) {
    & python $script @args
} elseif (Get-Command py -ErrorAction SilentlyContinue) {
    & py -3 $script @args
} else {
    Write-Error 'pr-review needs Python 3'
    exit 2
}
exit $LASTEXITCODE
