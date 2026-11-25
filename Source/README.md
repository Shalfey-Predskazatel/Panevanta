# Panevanta source

This folder contains the Panevanta 0.7.0 managed application, native D3D11/CUDA engine, optional interop probe, build script, and app assets.

- Panevanta: the Windows desktop application.
- Native: the D3D11, CUDA, RTX Video, and presentation engine.
- Interop: a separate D3D12/CUDA experiment; it is not used by the released application.
- Build-Panevanta.ps1: builds from a complete recovery bundle with the required dependency folders.

The public repository does not contain the offline toolchain archive or NVIDIA SDK payload. Read [the build guide](../docs/building.md) before attempting a rebuild.

## Русский

Здесь находятся приложение Panevanta 0.7.0 на C#, нативный движок D3D11/CUDA, отдельный interop-пример, скрипт сборки и ресурсы.

- Panevanta: приложение Windows.
- Native: движок D3D11, CUDA, RTX Video и вывода.
- Interop: отдельный эксперимент с D3D12/CUDA, который не используется в опубликованной программе.
- Build-Panevanta.ps1: сборка из полного комплекта восстановления с необходимыми папками зависимостей.

В публичном репозитории нет офлайн-архива инструментов и файлов NVIDIA SDK. Перед пересборкой прочитайте [инструкцию](../docs/building.ru.md).
