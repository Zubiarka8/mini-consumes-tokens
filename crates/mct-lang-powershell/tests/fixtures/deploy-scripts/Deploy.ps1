Import-Module Lib

$Version = "1.0.0"

function Invoke-Build {
  Write-Log "building $Version"
}

function Invoke-Deploy {
  Invoke-Build
  Write-Log "deployed"
}
