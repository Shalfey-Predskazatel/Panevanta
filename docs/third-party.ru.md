# Сторонние компоненты

[English](third-party.md) · **Русский**

Panevanta 0.9.7 объединяет оригинальный код проекта с компонентами NVIDIA, Microsoft и открытых проектов. Уведомления о сторонних компонентах входят в папку `Licenses` portable-релиза.

- На NVIDIA Video Effects SDK, VFG, CUDA и связанные библиотеки распространяются условия NVIDIA.
- Мост DLSS 5 NR использует загрузчик NVIDIA NGX и подписанную модель NVIDIA Neural Rendering. SDK и файлы модели сохраняют собственные условия лицензирования.
- На Microsoft Edge WebView2 и библиотеки Microsoft Visual C++ распространяются условия Microsoft. Portable-комплект может содержать дополнительный runtime WebView2 Fixed Version.
- В оболочке используются Tauri и `windows-rs` по лицензиям MIT / Apache-2.0. Интерфейс использует React, Tailwind CSS, Radix Primitives и связанные компоненты по их открытым лицензиям. Значки Lucide распространяются по ISC, шрифт IBM Plex Sans — по SIL Open Font License 1.1.

Подробный список компонентов и ссылки приведены в [Source/Rust/THIRD-PARTY.md](../Source/Rust/THIRD-PARTY.md). Версии зависимостей закреплены в `Source/Rust/Cargo.lock` и `Source/Rust/frontend/package-lock.json`.

Лицензия MIT в этом репозитории распространяется только на оригинальный исходный код Panevanta и скрипты сборки. Она не передаёт права на название продукта, иллюстрации, компоненты NVIDIA или Microsoft, SDK, модели и другие материалы третьих сторон. Перед распространением проверьте условия лицензии каждого компонента.


