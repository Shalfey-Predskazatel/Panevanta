[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Baseline,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$NativeDll,
    [string]$FixedWebView2,
    [string]$NrBridge,
    [string]$NrSdkLicense,
    [string]$NrRuntime
)

$ErrorActionPreference = 'Stop'
$baselineRoot = (Resolve-Path -LiteralPath $Baseline).ProviderPath
$outputRoot = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
if (Test-Path -LiteralPath $outputRoot) {
    throw "Choose an output directory that does not yet exist: $outputRoot"
}
$sdk = Join-Path $baselineRoot 'Dependencies/NVIDIA-VideoFX'
$program = Join-Path $baselineRoot 'Program'
if (!$NativeDll) { $NativeDll = Join-Path $program 'Panevanta.Native.dll' }
foreach ($file in @($NativeDll, (Join-Path $sdk 'bin/NVVideoEffects.dll'))) {
    if (!(Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing input file: $file" }
}
if ($NrBridge) {
    foreach ($file in $NrBridge, $NrSdkLicense) {
        if (!$file -or !(Test-Path -LiteralPath $file -PathType Leaf)) { throw 'Pass the NR bridge and its NVIDIA SDK license file.' }
    }
}
if ($NrRuntime) {
    if (!$NrBridge) { throw 'Pass the NR bridge with the runtime.' }
    $NrRuntime = (Resolve-Path -LiteralPath $NrRuntime).ProviderPath
    $signature = Get-AuthenticodeSignature -LiteralPath $NrRuntime
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(?:^|, )CN=NVIDIA Corporation(?:,|$)') {
        throw 'The portable NR runtime must have a valid NVIDIA signature.'
    }
}
if ($FixedWebView2) {
    $FixedWebView2 = (Resolve-Path -LiteralPath $FixedWebView2).ProviderPath
    if (!(Test-Path -LiteralPath (Join-Path $FixedWebView2 'msedgewebview2.exe') -PathType Leaf)) {
        throw 'FixedWebView2 must contain the extracted Microsoft WebView2 Fixed Version runtime.'
    }
}

Push-Location (Join-Path $PSScriptRoot 'frontend')
try {
    npm ci --no-audit --no-fund
    if ($LASTEXITCODE) { throw "npm ci failed with exit code $LASTEXITCODE" }
    npm run build
    if ($LASTEXITCODE) { throw "Vite build failed with exit code $LASTEXITCODE" }
}
finally { Pop-Location }

$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
cargo build --release --locked --target x86_64-pc-windows-msvc --manifest-path $manifest
if ($LASTEXITCODE) { throw "Cargo build failed with exit code $LASTEXITCODE" }
$metadata = cargo metadata --no-deps --format-version 1 --manifest-path $manifest | ConvertFrom-Json
$executable = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc/release/panevanta.exe'

$destination = Join-Path $outputRoot 'Program'
$dependencies = Join-Path $outputRoot 'Dependencies'
$licenses = Join-Path $outputRoot 'Licenses'
New-Item -ItemType Directory -Path $destination,$dependencies,$licenses | Out-Null
Copy-Item -LiteralPath $executable -Destination (Join-Path $destination 'Panevanta.exe')
Copy-Item -LiteralPath $NativeDll -Destination (Join-Path $destination 'Panevanta.Native.dll')
if ($NrBridge) {
    Copy-Item -LiteralPath $NrBridge -Destination (Join-Path $destination 'Panevanta.NR.dll')
    Copy-Item -LiteralPath $NrSdkLicense -Destination (Join-Path $licenses 'NVIDIA-DLSS-SDK-LICENSE.txt')
}
foreach ($name in 'msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll') {
    Copy-Item -LiteralPath (Join-Path $program $name) -Destination $destination
}
Copy-Item -LiteralPath $sdk -Destination $dependencies -Recurse
if ($NrRuntime) {
    $nrDestination = Join-Path $dependencies 'DLSS-NR'
    New-Item -ItemType Directory -Path $nrDestination | Out-Null
    Copy-Item -LiteralPath $NrRuntime -Destination (Join-Path $nrDestination 'nvngx_dlssnr.dll')
}
if ($FixedWebView2) {
    Copy-Item -LiteralPath $FixedWebView2 -Destination (Join-Path $dependencies 'WebView2') -Recurse
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'START-HERE.ru.md') -Destination $outputRoot
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../../docs/qa-0.8.ru.md') -Destination (Join-Path $outputRoot 'QA.ru.md')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../../LICENSE') -Destination $licenses
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY.md') -Destination $licenses
$modules = Join-Path $PSScriptRoot 'frontend/node_modules'
foreach ($module in @('react','react-dom','lucide-react','@fontsource-variable/ibm-plex-sans','@radix-ui/react-switch','@radix-ui/react-slot','@radix-ui/react-select','@radix-ui/react-dialog','class-variance-authority','clsx','tailwind-merge','tailwindcss','@tauri-apps/api')) {
    $notice = Get-ChildItem -LiteralPath (Join-Path $modules $module) -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING)(\..*)?$'
    foreach ($file in $notice) {
        $name = $module.Replace('/', '-').TrimStart('@') + '-' + $file.Name + '.txt'
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $licenses $name)
    }
}
Write-Output "Portable build: $outputRoot"
