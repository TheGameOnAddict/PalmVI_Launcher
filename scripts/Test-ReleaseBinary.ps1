[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Path
)

$ErrorActionPreference = 'Stop'
$binary = Get-Item -LiteralPath $Path
if ($binary.PSIsContainer -or $binary.Extension -ne '.exe') {
    throw 'Select the final Windows executable to scan.'
}

$status = Get-CimInstance -Namespace root/Microsoft/Windows/Defender -ClassName MSFT_MpComputerStatus
if (!$status.AntivirusEnabled -or !$status.AMServiceEnabled -or $status.DefenderSignaturesOutOfDate) {
    throw 'Enable Microsoft Defender Antivirus and update its security intelligence before publishing.'
}

$scanner = Join-Path $env:ProgramFiles 'Windows Defender/MpCmdRun.exe'
$platformScanner = Join-Path $env:ProgramData "Microsoft/Windows Defender/Platform/$($status.AMProductVersion)-0/MpCmdRun.exe"
if (Test-Path -LiteralPath $platformScanner) {
    $scanner = $platformScanner
}

# Keep the bytes stable while Defender scans and while computing the upload digest.
$stream = [IO.File]::Open($binary.FullName, 'Open', 'Read', 'Read')
try {
    & $scanner -Scan -ScanType 3 -File $binary.FullName -DisableRemediation | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "Release scan failed or found a threat (Defender exit code $LASTEXITCODE). Hold publication for review."
    }
    [pscustomobject]@{
        SHA256 = (Get-FileHash -InputStream $stream -Algorithm SHA256).Hash.ToLowerInvariant()
        SignatureVersion = $status.AntivirusSignatureVersion
    }
} finally {
    $stream.Dispose()
}
