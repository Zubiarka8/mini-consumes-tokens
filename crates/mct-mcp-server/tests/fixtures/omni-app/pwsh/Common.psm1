function Write-Line {
    param([string]$Message)
    Write-Output $Message
}

function New-Artifact {
    Write-Line -Message "building"
}
