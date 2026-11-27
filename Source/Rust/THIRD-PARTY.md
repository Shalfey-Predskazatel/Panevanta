# Third-party components in Panevanta 0.9.7

- NVIDIA Video Effects SDK, Video Frame Generation, CUDA and related redistributables retain the NVIDIA terms and files shipped under Dependencies.
- The Panevanta NR bridge links the NVIDIA NGX loader from the public DLSS SDK 310.9.16. Panevanta loads the NVIDIA-signed Neural Rendering model from Dependencies/DLSS-NR or the local system. The local portable package can include the signed model passed to the packaging script. Veyra-NRVideo and NeuralScreen served as references for model selection and texture interop.
- Microsoft Edge WebView2 is supplied by the system. An optional Fixed Version runtime in Dependencies/WebView2 is distributed under Microsoft's WebView2 terms. Microsoft Visual C++ redistributable libraries in Program retain their Microsoft terms.
- The desktop shell uses [Tauri](https://github.com/tauri-apps/tauri) (MIT / Apache-2.0) and [windows-rs](https://github.com/microsoft/windows-rs) (MIT / Apache-2.0).
- The interface uses [React](https://github.com/facebook/react), [Tailwind CSS](https://github.com/tailwindlabs/tailwindcss), [shadcn/ui](https://github.com/shadcn-ui/ui), and [Radix Primitives](https://github.com/radix-ui/primitives) (MIT). Local shadcn/ui components are adapted for Panevanta.
- [Lucide](https://github.com/lucide-icons/lucide) icons are licensed under ISC with upstream Feather terms. IBM Plex Sans is distributed under SIL Open Font License 1.1.
- Panevanta branding and assets originate in the Panevanta repository and use its branding terms. The palette, window controls and visual treatment draw on the author's Codex Accounts r85 material.

The build locks dependency versions in Cargo.lock and frontend/package-lock.json.
