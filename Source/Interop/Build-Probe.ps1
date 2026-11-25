param(
    [Parameter(Mandatory)]
    [string]$WorkDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$buildFiles = Join-Path $root 'Dependencies/Build'
$sdk = Join-Path $root 'Dependencies/NVIDIA-VideoFX'
$work = [IO.Path]::GetFullPath($WorkDirectory)
$msys = Join-Path $work 'msys64'
$clang = Join-Path $msys 'clang64/bin'
$headers = Join-Path $work 'cuda-headers'
$wheel = Join-Path $buildFiles 'nvidia_cuda_runtime_cu12-12.8.90-py3-none-win_amd64.whl'
New-Item -Type Directory -Path $work, $msys, $headers -Force | Out-Null
if (!(Test-Path -LiteralPath (Join-Path $clang 'clang++.exe'))) {
    & (Join-Path $env:WINDIR 'System32/tar.exe') -xf (Join-Path $buildFiles 'MSYS2-clang64.tar.gz') -C $msys
    if ($LASTEXITCODE) { throw "MSYS2 extraction failed ($LASTEXITCODE)." }
}
$env:PATH = "$clang;$sdk/bin;$sdk/features/nvvfxvideoframegeneration/bin;$env:PATH"
if (!(Test-Path -LiteralPath (Join-Path $headers 'nvidia/cuda_runtime/include/cuda.h'))) {
    & (Join-Path $clang 'python.exe') -m zipfile -e $wheel $headers
    if ($LASTEXITCODE) { throw "CUDA header extraction failed ($LASTEXITCODE)." }
}
$exe = Join-Path $work 'panevanta-gpu-probe.exe'
$arguments = @('-municode', '-std=c++20', '-O2', '-Wall', '-Wextra',
    '-I', (Join-Path $headers 'nvidia/cuda_runtime/include'),
    '-I', (Join-Path $sdk 'nvvfx/include'), (Join-Path $PSScriptRoot 'Probe.cpp'),
    '-o', $exe, '-ld3d12', '-ldxgi', (Join-Path $sdk 'bin/NVCVImage.dll'))
& (Join-Path $clang 'clang++.exe') @arguments
if ($LASTEXITCODE) { throw "Probe compilation failed ($LASTEXITCODE)." }
& $exe
if ($LASTEXITCODE) { throw "GPU transfer test failed ($LASTEXITCODE)." }
& $exe $sdk --bench
if ($LASTEXITCODE) { throw "BGRA8 VFG test failed ($LASTEXITCODE)." }
& $exe $sdk --hdr10 --bench
if ($LASTEXITCODE) { throw "RGB10A2 VFG test failed ($LASTEXITCODE)." }
