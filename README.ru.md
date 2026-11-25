![Panevanta by Shalfey Predskazatel](assets/social-preview.png)

# Panevanta

[English](README.md) · **Русский**

**Portable-утилита для Windows, которая добавляет генерацию кадров NVIDIA, RTX Video Super Resolution и HDR выбранному окну или экрану.**

[Скачать Panevanta 0.7 для Windows x64](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Portable-win-x64.zip) · [Исходный код](https://github.com/Shalfey-Predskazatel/Panevanta/tree/main/Source) · [Архив исходников](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip) · [Релиз и SHA-256](https://github.com/Shalfey-Predskazatel/Panevanta/releases/tag/v0.7)

Panevanta захватывает выбранное окно или монитор и показывает обработанное изображение в кликабельном overlay-слое. Декодирование видео и звук остаются у плеера или браузера.

- NVIDIA Video Frame Generation, RTX Video Super Resolution и RTX Video HDR.
- Windows Graphics Capture, Direct3D 11 и обработка на CUDA.
- Для выбранных эффектов нужна совместимая видеокарта NVIDIA RTX и драйвер GeForce.
- В portable-релиз входят файлы среды выполнения, уведомления сторонних лицензий и русская краткая инструкция.

Распакуйте ZIP целиком и запустите `Program/Panevanta.exe`. Выберите окно или экран и нажмите **Включить RTX**. Комбинация по умолчанию — **Shift + Alt + S**.

## Исходники и сборка

В репозитории есть исходный код Panevanta 0.7, нативного движка, скрипты сборки и документация. Откройте папку [Source](Source/) или скачайте [архив исходников](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip).

Кэш офлайн-инструментов и SDK размером 3,1 ГБ из комплекта восстановления не загружается в GitHub. Для пересборки нужны зафиксированный .NET toolchain, MSYS2 CLANG64, заголовки CUDA и NVIDIA Video Effects SDK, полученный с соблюдением лицензии. См. [инструкцию по сборке](docs/building.ru.md) и [заметки о сторонних компонентах](docs/third-party.md).

## Лицензия

Оригинальный исходный код Panevanta и скрипты сборки распространяются по [лицензии MIT](LICENSE). Она не распространяется на название продукта, логотипы, иллюстрации и сторонние компоненты. Для NVIDIA, Microsoft, .NET, CUDA и других включённых или необходимых компонентов действуют их собственные условия; см. [заметки о сторонних компонентах](docs/third-party.md).

## Примечания

Overlay не декодирует видео и не обрабатывает звук. Частота захвата или подачи кадров в DXGI не обязательно равна физической частоте монитора и не подтверждает синхронизацию со звуком.

Panevanta — независимый проект, не связанный с NVIDIA или Microsoft и не одобренный ими. Названия продуктов используются для описания совместимости.

by Shalfey Predskazatel
