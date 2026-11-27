[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ClangBin,
    [Parameter(Mandatory)][string]$CudaInclude,
    [Parameter(Mandatory)][string]$SdkRoot,
    [Parameter(Mandatory)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$native = Join-Path $PSScriptRoot '../Native'
$vfxInclude = Join-Path $SdkRoot 'nvvfx/include'
$outputRoot = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($outputRoot)
$obj = Join-Path $outputRoot 'imageview.o'
$output = Join-Path $outputRoot 'Panevanta.Native.dll'

& (Join-Path $ClangBin 'clang.exe') -O2 -I $vfxInclude -c (Join-Path $native 'ImageView.c') -o $obj
if ($LASTEXITCODE) { throw "ImageView compilation failed with exit code $LASTEXITCODE" }
& (Join-Path $ClangBin 'clang++.exe') '-std=c++20' -O2 -Wall -Wextra -shared '-I' $CudaInclude '-I' $vfxInclude (Join-Path $native 'Engine.cpp') $obj '-o' $output '-ld3d11' '-ld3dcompiler' '-ldxgi' '-ldxguid' '-static'
if ($LASTEXITCODE) { throw "Native engine compilation failed with exit code $LASTEXITCODE" }
Get-Item -LiteralPath $output
