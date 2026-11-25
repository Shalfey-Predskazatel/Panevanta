param([Parameter(Mandatory)][string]$WorkDirectory)

$ErrorActionPreference = 'Stop'
$bundle = Split-Path -Parent $PSScriptRoot
$tools = Join-Path $bundle 'Dependencies/Build'
$sdk = Join-Path $bundle 'Dependencies/NVIDIA-VideoFX'
$work = [IO.Path]::GetFullPath($WorkDirectory)
if ($work -eq $bundle -or $work.StartsWith($bundle + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Use a separate work directory outside the backup.'
}
$tar = Join-Path $env:WINDIR 'System32/tar.exe'
function Invoke-Checked($program, [string[]]$arguments) {
    & $program @arguments
    if ($LASTEXITCODE) { throw "$program exited with code $LASTEXITCODE" }
}
New-Item -ItemType Directory -Path $work -Force | Out-Null
$dotnetRoot = Join-Path $work 'dotnet'
$msys = Join-Path $work 'msys64'
$clang = Join-Path $msys 'clang64/bin'
if (!(Test-Path -LiteralPath (Join-Path $dotnetRoot 'sdk/8.0.428/dotnet.dll'))) {
    New-Item -Type Directory -Path $dotnetRoot -Force | Out-Null
    Invoke-Checked $tar @('-xf', (Join-Path $tools 'dotnet-sdk-8.0.428-win-x64.tar.gz'), '-C', $dotnetRoot)
}
if (!(Test-Path -LiteralPath (Join-Path $clang 'clang++.exe'))) {
    New-Item -Type Directory -Path $msys -Force | Out-Null
    Invoke-Checked $tar @('-xf', (Join-Path $tools 'MSYS2-clang64.tar.gz'), '-C', $msys)
}
$headers = Join-Path $work 'cuda-headers'
if (!(Test-Path -LiteralPath (Join-Path $headers 'nvidia/cuda_runtime/include/cuda.h'))) {
    $wheel = Join-Path $tools 'nvidia_cuda_runtime_cu12-12.8.90-py3-none-win_amd64.whl'
    Invoke-Checked (Join-Path $clang 'python.exe') @('-m', 'zipfile', '-e', $wheel, $headers)
}
$env:DOTNET_ROOT = $dotnetRoot
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:NUGET_PACKAGES = Join-Path $work 'nuget-packages'
$env:PATH = "$clang;$env:PATH"
$dotnet = Join-Path $dotnetRoot 'dotnet.exe'
$program = Join-Path $work 'Program'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Panevanta') -Destination $work -Recurse -Force
$project = Join-Path $work 'Panevanta/Panevanta.csproj'
Invoke-Checked $dotnet @('restore', $project, '-r', 'win-x64', '-p:SelfContained=true',
    '--source', (Join-Path $tools 'NuGet'), '-p:NuGetAudit=false')
Invoke-Checked $dotnet @('publish', $project, '--no-restore', '-c', 'Release', '-r', 'win-x64',
    '--self-contained', 'true', '-p:DebugType=None', '-o', $program)
$obj = Join-Path $work 'imageview.o'
$output = Join-Path $program 'Panevanta.Native.dll'
$cudaInclude = Join-Path $headers 'nvidia/cuda_runtime/include'
$vfxInclude = Join-Path $sdk 'nvvfx/include'
$native = Join-Path $PSScriptRoot 'Native'
Invoke-Checked (Join-Path $clang 'clang.exe') @('-O2', '-I', $vfxInclude, '-c',
    (Join-Path $native 'ImageView.c'), '-o', $obj)
Invoke-Checked (Join-Path $clang 'clang++.exe') @('-std=c++20', '-O2', '-Wall', '-Wextra', '-shared',
    '-I', $cudaInclude, '-I', $vfxInclude, (Join-Path $native 'Engine.cpp'), $obj,
    '-o', $output, '-ld3d11', '-ld3dcompiler', '-ldxgi', '-ldxguid', '-static')
foreach ($name in 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll', 'THIRD-PARTY.md') {
    Copy-Item -LiteralPath (Join-Path $bundle "Program/$name") -Destination $program -Force
}
Write-Output "Build output: $program"
Write-Output 'Copy Dependencies/NVIDIA-VideoFX beside Program to run VFG.'
