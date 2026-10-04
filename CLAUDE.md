# CLAUDE.md

Fork personal de Hydra Launcher: añade `hydra-native` (Rust), Steam Input, modo SISR y proveedores de bibliotecas.

## Tech Stack
Electron · React · TypeScript · Tailwind · Rust (`native/hydra-native`, addon nativo) · Yarn.

## Comandos
- Instalar: `yarn install` (el `postinstall` compila el addon Rust)
- Desarrollo: `yarn dev`
- Compilar app: `yarn build` (typecheck + electron-vite)
- Compilar Rust: `yarn build:native` (o `cargo build --release` en `native/hydra-native`)
- Tests: `yarn test` · Rust: `cargo test` en `native/hydra-native`
- Calidad: `yarn typecheck` · `yarn lint` · `yarn format-check`

## Arquitectura y reglas del fork
- **Respetar el upstream**: minimizar cambios en código existente; poner la lógica nueva en módulos propios y tocar archivos de Hydra solo con puntos de enganche mínimos (facilita merges con upstream).
- **Seguridad de datos**: antes de modificar `shortcuts.vdf`, hacer backup; tras escribir, reparsear el resultado para validarlo y restaurar el backup si falla.
- **TypeScript**: tipado estricto; sin `any` ni `@ts-ignore` injustificados.
- **Rust**: no usar `unwrap()`/`expect()` sin control en IO ni en parseo; propagar con `Result` y convertir errores a mensajes útiles hacia JS.

## Commits y ramas
- Ramas: `feat/<tema>`, `fix/<tema>`, `chore/<tema>`, `refactor/<tema>`.
- Commits: Conventional Commits (`feat:`, `fix:`, `chore:`…), validados por commitlint/husky; asunto corto en imperativo.
- Nunca commitear directo a `main`. Sin PRs: pushear la rama e integrarla en `develop` con merge (o rebase + fast-forward) directamente.
