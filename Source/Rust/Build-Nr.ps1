[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SdkRoot,
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$sdk = (Resolve-Path -LiteralPath $SdkRoot).ProviderPath
$include = Join-Path $sdk 'include'
$ngx = Join-Path $sdk 'lib/Windows_x86_64/x64/nvsdk_ngx_d.lib'
if (!(Test-Path -LiteralPath (Join-Path $include 'nvsdk_ngx.h')) -or !(Test-Path -LiteralPath $ngx)) {
    throw 'Use the extracted Windows x64 NVIDIA DLSS SDK.'
}

$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($output)
$source = Join-Path $PSScriptRoot '../Native/DlssNr.cpp'
& cl.exe /nologo /LD /O2 /EHsc /MD /std:c++17 "/I$include" "/Fo$output\DlssNr.obj" $source $ngx d3d11.lib d3d12.lib dxgi.lib advapi32.lib user32.lib d3dcompiler.lib wintrust.lib crypt32.lib version.lib /link "/OUT:$output\Panevanta.NR.dll" "/IMPLIB:$output\Panevanta.NR.lib"
if ($LASTEXITCODE) { throw "DLSS NR bridge build failed with exit code $LASTEXITCODE" }
Get-Item -LiteralPath (Join-Path $output 'Panevanta.NR.dll')
