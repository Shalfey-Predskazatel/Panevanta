![Panevanta by Shalfey Predskazatel](assets/panevanta-cover.png)

# Panevanta

**A portable Windows tool that adds NVIDIA frame generation, RTX Video Super Resolution, and HDR to a selected window or display.**

[Download Panevanta 0.7 for Windows x64](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Portable-win-x64.zip) · [Source code](https://github.com/Shalfey-Predskazatel/Panevanta/tree/main/Source) · [Source archive](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip) · [Release and SHA-256](https://github.com/Shalfey-Predskazatel/Panevanta/releases/tag/v0.7) · [Русский](#русский)

Panevanta captures a selected window or display and shows the processed image in a click-through overlay. The player or browser continues to decode video and play audio.

- NVIDIA Video Frame Generation, RTX Video Super Resolution, and RTX Video HDR.
- Windows Graphics Capture, Direct3D 11, and CUDA processing.
- Requires a compatible NVIDIA RTX GPU and GeForce driver for the selected effects.
- The portable release includes the runtime files, third-party notices, and Russian quick-start guide.

Extract the complete portable ZIP and run Program/Panevanta.exe. Select a window or display and enable RTX. The default shortcut is **Shift + Alt + S**.

## Source and build

This repository now includes the Panevanta 0.7 application sources, native engine sources, installer/build scripts, and documentation. Browse [Source](Source/) or download the [source archive](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip).

The 3.1 GB offline toolchain and SDK cache from the recovery bundle is not stored in GitHub. A rebuild needs the pinned .NET toolchain, MSYS2 CLANG64, CUDA headers, and a properly licensed NVIDIA Video Effects SDK. See [Building](docs/building.md) and [third-party notes](docs/third-party.md).

The source is published for inspection. No reuse license is specified for the project code. Third-party components keep their own license terms.

## Notes

The overlay does not decode video or handle audio. Frame rate shown by the app represents capture or DXGI presentation, not necessarily the display's physical refresh rate or audio synchronization.

Panevanta is an independent project and is not affiliated with or endorsed by NVIDIA or Microsoft. Product and company names identify compatible technologies.

---

## Русский

**Portable-утилита для Windows, которая добавляет генерацию кадров NVIDIA, RTX Video Super Resolution и HDR выбранному окну или экрану.**

[Скачать Panevanta 0.7 для Windows x64](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Portable-win-x64.zip) · [Исходный код](https://github.com/Shalfey-Predskazatel/Panevanta/tree/main/Source) · [Архив исходников](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip) · [Релиз и SHA-256](https://github.com/Shalfey-Predskazatel/Panevanta/releases/tag/v0.7)

Panevanta захватывает выбранное окно или монитор и показывает обработанное изображение в кликабельном overlay-слое. Декодирование видео и звук остаются у плеера или браузера.

- NVIDIA Video Frame Generation, RTX Video Super Resolution и RTX Video HDR.
- Windows Graphics Capture, Direct3D 11 и обработка на CUDA.
- Для выбранных эффектов нужна совместимая видеокарта NVIDIA RTX и драйвер GeForce.
- В portable-релиз входят файлы среды выполнения, уведомления сторонних лицензий и русская краткая инструкция.

Распакуйте ZIP целиком и запустите Program/Panevanta.exe. Выберите окно или экран и нажмите **Включить RTX**. Комбинация по умолчанию — **Shift + Alt + S**.

## Исходники и сборка

В репозитории есть исходный код Panevanta 0.7, нативного движка, скрипты сборки и документация. Откройте папку [Source](Source/) или скачайте [архив исходников](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip).

Кэш офлайн-инструментов и SDK размером 3,1 ГБ из комплекта восстановления не загружается в GitHub. Для пересборки нужны зафиксированный .NET toolchain, MSYS2 CLANG64, заголовки CUDA и NVIDIA Video Effects SDK, полученный с соблюдением лицензии. См. [инструкцию по сборке](docs/building.ru.md) и [заметки о сторонних компонентах](docs/third-party.md).

Исходный код доступен для ознакомления. Лицензия на повторное использование проекта не указана. Для сторонних компонентов действуют их собственные лицензии.

## Примечания

Overlay не декодирует видео и не обрабатывает звук. Частота захвата или подачи кадров в DXGI не обязательно равна физической частоте монитора и не подтверждает синхронизацию со звуком.

Panevanta — независимый проект, не связанный с NVIDIA или Microsoft и не одобренный ими. Названия продуктов используются для описания совместимости.

by Shalfey Predskazatel
