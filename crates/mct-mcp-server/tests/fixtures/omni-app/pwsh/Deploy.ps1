Import-Module ./Common.psm1

function Invoke-Deploy {
    New-Artifact
    Write-Line -Message "deployed"
}

Invoke-Deploy
