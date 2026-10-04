# PLAN.md — Fork de Hydra: Steam Input · SISR · Library Providers

> Estado: análisis y spikes. Ningún código de producción modificado. Spikes en `./spikes/` (excluido vía `.git/info/exclude`).

## 1. Resumen ejecutivo

Objetivo: convertir Hydra en un orquestador local de biblioteca (estilo Playnite + SISR Helper) que registre, lance y aplique plantillas comunitarias de Steam Input a cualquier juego (Steam o no) con el Steam Controller (2026).

**Decisiones tomadas con el usuario**
- **Steam opcional (híbrido).** Verificado en el código de SISR (`docs/guides/no_steam.md`): ni SISR ni el Helper de Playnite eliminan Steam. `--no-steam` solo traduce mando físico → Xbox360/DS4, **sin remapeo ni plantillas**. Hydra funciona completo sin Steam; con Steam detectado se habilitan Steam Input y plantillas.
- **Reinicio de Steam**: se evita siempre que se pueda; si es inevitable, **reinicio silencioso confirmado** (`steam -shutdown` → escribir+validar → `steam -silent`).
- **Códec VDF binario en Rust**, en un crate puro (`native/steam-vdf`) consumido por `hydra-native`.

**Arquitectura en 3 capas** (toda la lógica nueva en módulos propios: `src/main/fork/**`, `src/preload/fork.ts`, `src/renderer/src/fork/**`, `native/steam-vdf`; upstream solo se toca con ~12 hooks de una línea):
1. **Steam Presence** (opcional): detección, usuarios, reinicio confirmado, cola de cambios.
2. **Controller Runtime** por juego, con tres modos: `steam-shortcut` (el juego es un shortcut de Steam, `rungameid`), `sisr-steam` (shortcut `SISR - <Juego>` lleva el layout; el juego se lanza directo), `sisr-nosteam` (SISR `--no-steam` + mapeos SDL3, sin Steam).
3. **Library Providers**: Steam (adaptador sobre código upstream), Epic, GOG (+ Heroic en Linux), con deduplicación.

**Resultados de spikes ejecutados (macOS)**: P1 y P2 confirman que un códec Rust propio es lossless byte a byte y que la librería Node que usa upstream **no** lo es (corrompe índices dispersos). Ver §3.

## 2. Mapa del repo

### Procesos
- **main**: `src/main/index.ts`; `loadState()` en `src/main/main.ts:63` importa `./events` (:79).
- **preload**: `src/preload/index.ts` (`exposeInMainWorld("electron")` :138, `contextIsolation` activo, `sandbox:false`).
- **renderer**: `src/renderer/src/main.tsx` (HashRouter + Redux Toolkit). Big Picture (`src/big-picture`) fuera de alcance del MVP.

### Persistencia
- LevelDB (`classic-level`) en `userData/hydra-db`. Juegos: `gamesSublevel`, clave `${shop}:${objectId}`. `UserPreferences` clave raíz (`src/types/level.types.ts:142`).
- `Game` (`level.types.ts:31-88`) ya tiene `steamShortcutAppId`, `launchOptions`, `winePrefixPath`. `GameShop = "steam"|"custom"|"launchbox"`.

### IPC
- `registerEvent` (`src/main/events/register-event.ts`) envuelve `ipcMain.handle` y clona el resultado a JSON. ~256 eventos agrupados por carpeta e importados en `src/main/events/index.ts`.
- Push a renderer: `WindowManager.sendToAppWindows`. Tipos del renderer escritos a mano en `src/renderer/src/declaration.d.ts:137`.

### `hydra-native`
- napi-rs 3 (cdylib). `scripts/build-native-addon.cjs` copia a `hydra-native/hydra-native.node` (`extraResources`). Loader perezoso `src/main/services/native-addon.ts` con tipo manual `HydraNativeModule`. Errores con `Error::from_reason`.
- El build depende de libtorrent/vcpkg (`HYDRA_TORRENT_LIB_DIR`) → **no compila aquí**; por eso el códec va en crate puro. ~34 `#[test]`, `cargo test` no corre en CI.

### Alta y lanzamiento
- Alta: `addCustomGameToLibrary` (juego custom, UUID).
- Lanzamiento: `openGame` → `launchGame` → `launchGameWithCloudSaveChecks` (`src/main/helpers/launch-game.ts:597`). Ya soporta `steamProtocolLaunch` con fallback nativo (:608, :804).
- Seguimiento: `process-watcher.ts` casa por `executablePath` → el playtime sigue funcionando aunque Steam lance el proceso; cierre en `onCloseGame` (:571).

### Código Steam existente
- `src/main/services/steam.ts`: ubicación, usuarios, `getSteamShortcuts`/`writeSteamShortcuts` (**sin backup ni validación → viola CLAUDE.md**), appid CRC, `composeSteamShortcut`.
- `events/library/create-steam-shortcut.ts`: copia arte grid por appid; dedupe por `appname`.
- `steam-integration/*`: sync de cuenta, `appmanifest_*.acf`, `appinfo.vdf` binario, `loginusers.vdf`.
- **No existe** Steam Input, SISR, Epic, GOG ni abstracción de providers.

### Piezas reutilizables
`spawn-detached-emulator.ts` + `emulator-session-tracker.ts` (mapa de sesiones, kill por OS), `NativeAddon.listProcesses`, limpieza en `index.ts:390` (`Promise.allSettled`), `useGameOptionsModal`, pestaña Integraciones (`settings-context-integrations.tsx:11`).

### Hooks de bajo acoplamiento

| # | Archivo upstream | Cambio |
|---|---|---|
| H1 | `src/main/events/index.ts` | `import "./fork";` |
| H2 | `src/preload/index.ts` | `import "./fork";` (expone `window.hydraFork` aparte) |
| H3 | `src/main/helpers/launch-game.ts:608` | `steamProtocolLaunch = shop==="steam" ? … : await ForkLaunch.resolveOverride(game, …)` |
| H4 | mismo archivo, antes del dispatch | `await ForkLaunch.beforeLaunch(game)` (SISR) |
| H5 | `src/main/services/process-watcher.ts:571` | `void ForkLaunch.afterExit(game)` |
| H6 | `src/main/index.ts:~398` | `ForkLaunch.shutdown()` en `allSettled` |
| H7 | `src/main/services/steam.ts:259` | `writeSteamShortcuts` delega en `SafeShortcutsStore` |
| H8 | `native/hydra-native/{Cargo.toml,src/lib.rs}` | dependencia path + `mod fork_steam;`; tipos aditivos en `native-addon.ts` |
| H9 | `hero-panel-actions.tsx:~419` | `<ForkGameControls/>` |
| H10 | `settings-context-integrations.tsx:11` | `<ForkIntegrationsSettings/>` |
| H11 | `components/header/header.tsx:~434` | `<ForkImportButton/>` |
| H12 | `src/types/level.types.ts` | campos opcionales `fork*` en `UserPreferences` |

## 3. Matriz de validación

| Hipótesis | Estado | Hallazgo |
|---|---|---|
| `.vscode/` excluido de git | VERIFICADO | `.gitignore:1`; `git ls-files .vscode` vacío. Sin cambios necesarios. |
| Hydra ya lee/escribe `shortcuts.vdf` | VERIFICADO | `steam.ts:192-275` con `steam-shortcut-editor`. |
| `steam-shortcut-editor` (upstream) es lossless | **VERIFICADO: NO** | Spike P1: round-trip OK con 13 B y 200 entradas, pero con índices dispersos (0,5,99) 374 B → **851 B** (rellena huecos). Además sin `default` en el `switch` (tipo desconocido desincroniza). |
| Un códec Rust lossless hace round-trip byte a byte | VERIFICADO | P1 (`spikes/vdf-codec`): 13 B real, 200 entradas (UTF-8, comillas, espacios), índices dispersos → idénticos; tipo 0x07 y truncado → error con offset; backup→editar `AppName`→escribir tmp+rename→reparsear→comparar: OK. |
| `steam_shortcuts_util` 1.1.9 sirve como códec | VERIFICADO: NO como base | Parsea 200 y 3 dispersas, pero su reescritura **no** es idéntica (modelo de campos fijo). Útil solo como referencia. |
| Flujo backup + reparseo + restauración | VERIFICADO | P1 implementa y valida el patrón exigido por CLAUDE.md. |
| Fórmula CRC32(exe+name)\|0x80000000 es calculable de forma consistente | VERIFICADO (cálculo) | P2: 3 implementaciones (`zlib.crc32`, tabla propia, fórmula upstream) coinciden; check value `cbf43926` OK; bit alto siempre 1. |
| El appid CRC32 coincide con el que asigna Steam | DESCARTADO como requisito | El Helper halló appids aleatorios en entradas reales. Decisión: escribir appid propio (alto bit 1), releerlo, **nunca recalcular**. |
| `rungameid = (appid<<32)\|0x02000000` | VERIFICADO (aritmética) / REQUIERE SPIKE (W2) | P2 calcula con BigInt; falta confirmar que Steam lo lanza. |
| Steam solo lee `shortcuts.vdf` al arrancar y puede sobrescribirlo | REQUIERE SPIKE (W1) | AGENTS.md del Helper; mismo límite en GlosSI. |
| El `appid` escrito se respeta (rungameid/grid) | REQUIERE SPIKE (W2) | Upstream copia arte con ese appid. |
| `steam://controllerconfig/<appid>/<workshopId>` aplica layout sin UI | REQUIERE SPIKE (W3) | El URI existe; probable que abra configurador con botón "Aplicar". |
| Editar `configset_controller_*.vdf` con Steam cerrado aplica plantilla | REQUIERE SPIKE (W4) | Riesgo: Steam Cloud (app 241100). Rutas ausentes en macOS → Windows. |
| Plantillas consultables vía `IPublishedFileService/QueryFiles` (app 241100) | REQUIERE SPIKE (W6) | `steam-web-api.ts` ya firma con `access_token`. |
| SISR requiere Steam en modo normal | VERIFICADO | FAQ SISR. |
| `--no-steam` = traducción, sin remapeo | VERIFICADO | `docs/guides/no_steam.md`. |
| SISR no admite 2 instancias | VERIFICADO (Helper) | VIIPER usa puertos fijos. |
| TOML de SISR solo admite claves planas / punteadas entrecomilladas | VERIFICADO (Helper v0.6.0) | Tablas anidadas → "unknown configuration keys". |
| SISR descubre configs en su propio directorio | VERIFICADO | Los TOML por juego deben vivir en `userData/fork/sisr/`. |
| SDL3 soporta Steam Controller 2026 (Triton) | VERIFICADO (SDL upstream) / REQUIERE SPIKE (W5) | `SDL_hidapi_steam_triton.c`; falta confirmar el SDL embebido en SISR. |
| No hay Epic/GOG/provider en Hydra | VERIFICADO | grep; solo escaneo/sync de Steam. |
| Toolchain Rust local operativa para spikes | VERIFICADO con caveat | `cargo` no está en PATH; con SDK macOS 27 el linker falla (`tapi error`) → usar `SDKROOT=…/MacOSX26.5.sdk`. |

Protocolo de los spikes de Windows (W1–W6): `spikes/windows/README.md`.

## 4. Matriz de riesgos y casos límite

| Riesgo | Mitigación |
|---|---|
| Steam sobrescribe `shortcuts.vdf` | Escribir solo con Steam cerrado; cola `forkPendingSteamChanges`; backups rotatorios `shortcuts.vdf.hydra.<ts>.bak` (máx. 5); tmp + fsync + rename; reparsear y comparar; restaurar ante fallo. |
| Corrupción por el códec Node upstream | Reemplazar escritura (H7) por códec Rust lossless (P1 lo demuestra). |
| Steam Cloud revierte layouts | `shortcuts.vdf` es local (verificar ausencia en `remotecache.vdf`); configset se verifica tras arrancar; fallback a flujo UI. |
| Rutas | `Exe`/`StartDir` siempre entrecomillados; UTF-8; normalizar con `path.win32` (insensible a mayúsculas) para dedupe; TOML con cadenas literales; launch options vía `parse-launch-options` upstream. |
| Permisos | Comprobar existencia/ejecutable (`X_OK` en Linux); SISR/VIIPER puede requerir admin para USBIP: detectar e informar, nunca escalar en silencio; `child.kill()` en Windows no mata el árbol → `taskkill /T` solo sobre PID propio. |
| Launchers intermedios (Epic/GOG/Ubisoft) | Rastrear el exe real (`trackingExecutablePaths`); override por URI en juegos con DRM; SISR vive toda la sesión, cierre al salir el proceso rastreado + periodo de gracia. |
| Colisión de IDs | Revisar appids de todos los usuarios; bit alto evita colisión con apps reales; nunca recalcular ids existentes; identificar nuestros shortcuts por marca en `LaunchOptions`/tag `hydra-fork:<gameKey>`, no por nombre. |
| Varios usuarios Steam | `userdata/*` + `loginusers.vdf` `MostRecent`; el usuario elige destino (por defecto, activo). |
| Identidad de SISR | PID + hora de inicio + línea de comandos con nuestro config; un SISR del usuario se adopta, nunca se mata. |
| Interferencia con `createSteamShortcut` upstream | Dedupe upstream es por `appname`; usar marcas propias. |
| Build nativo | Crate puro `native/steam-vdf` testeable sin vcpkg; `hydra-native` solo lo envuelve. |
| Merges con upstream | Máx. 12 hooks de una línea; resto en carpetas `fork/`. |

## 5. Arquitectura propuesta

### main (`src/main/fork/`)
- `steam/steam-presence.ts`: ¿instalado? ¿en ejecución? (`NativeAddon.listProcesses`); exe (registro `SteamExe` / Linux / Flatpak); usuarios.
- `steam/steam-restart.ts`: `-shutdown`, esperar salida, aplicar cola, `-silent`.
- `steam/safe-shortcuts-store.ts`: regla de seguridad de CLAUDE.md.
- `steam-input/steam-input-service.ts`: config por juego, shortcut, rungameid.
- `steam-input/template-catalog/`: interfaz `TemplateCatalogProvider` con `SteamWorkshopCatalog` (W6), `LocalTemplatesCatalog` (`controller_base/templates`), `ManualCatalog` (ID/URL pegado). Caché en sublevel `forkSteamInputTemplates` con TTL. Mapeo juego→AppID: `objectId` si `shop==="steam"`; si no, asociación manual/búsqueda en `forkSteamAppLinks`.
- `sisr/sisr-locator.ts` (`%LOCALAPPDATA%\SISR\SISR.exe`, AppImage, ruta configurable), `sisr-config.ts` (generador TOML plano; `userData/fork/sisr/<shop>-<objectId>.toml`), `sisr-session.ts` (modelo: `emulator-session-tracker.ts`).
- `launch/fork-launch.ts`: `resolveOverride` / `beforeLaunch` / `afterExit` / `shutdown`; siempre captura errores: un fallo del fork nunca bloquea un lanzamiento.
- `library/`: `LibraryProvider { id; isAvailable(); scan(signal): DetectedGame[] }`. Providers: `steam` (envuelve `scanInstalledGames`), `epic` (`%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`), `gog` (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\*` + `goggame-*.info`), Heroic en Linux. `dedup.ts`: (1) enlace externo, (2) appid Steam, (3) `executablePath` normalizado, (4) título + installDir → confirmación manual. Import como `shop:"custom"` + `forkLibraryLinks` (no se amplía `GameShop`).
- `platform/`: `PlatformAdapter` (Windows primario; Linux/Bazzite secundario: Steam Flatpak, `CompatToolMapping`).

### Rust
- `native/steam-vdf` (crate puro): `binary::{parse, write}` (árbol ordenado lossless, errores con offset, `thiserror`, sin `unwrap`), `text::{parse, write}` (configset/`.acf`), helpers de shortcut id. Base: `spikes/vdf-codec`.
- `hydra-native/src/fork_steam.rs`: `steamShortcutsRead(path)`, `steamShortcutsWrite(path, json)` (reparsea internamente), `steamVdfTextParse/Write`, `processCommandLine(pid)`. Errores con contexto (ruta, offset).

### IPC (prefijo `fork:`, `window.hydraFork`)
`fork:steam:{getStatus,listUsers,applyPendingChanges}`, `fork:steamInput:{getGameConfig,setGameConfig,searchTemplates,applyTemplate}`, `fork:sisr:{getStatus,setGameMode,openConfig}`, `fork:library:{listProviders,scan,import}`. Push: `fork:steam:pending-changed`, `fork:sisr:session-changed`, `fork:library:scan-progress`.

### Renderer (`src/renderer/src/fork/`)
`ForkGameControls` (botón hero + popover "Mando": modo, plantilla, estado), `TemplatePicker`, `ForkIntegrationsSettings` (Steam detectado/usuario, ruta SISR, toggles, cambios pendientes + "Aplicar (reinicia Steam)"), `ForkImportModal` (providers, preview con dedupe, importar). i18n: claves `fork` en `src/locales/{en,es}/translation.json`.

## 6. Roadmap de PRs atómicos

Todos desde rama `feat/*` (o `chore/*`), Conventional Commits, PR por rama, nunca a `main` directo.

0. **`chore/fork-plan`** — `CLAUDE.md` + `PLAN.md`. *DoD*: commitlint pasa.
1. **`feat/steam-vdf-codec`** — crate + napi + tipos en `native-addon.ts`. *DoD*: `cargo test -p steam-vdf` verde (round-trip byte a byte con fixtures reales y sintéticos —incl. índices dispersos—, error en tipo desconocido, sin `unwrap`/`expect` en IO/parseo); `yarn build:native` + `yarn typecheck` OK.
2. **`feat/safe-shortcuts-store`** — backup, tmp+rename, reparseo, restauración; H7. *DoD* (node:test con tmpdirs): escritura corrupta restaura original; máx. 5 backups; `createSteamShortcut` upstream sigue funcionando.
3. **`feat/steam-presence`** — detección + reinicio silencioso + cola. *DoD*: tests unitarios con mocks; checklist manual Windows (reinicio con confirmación; no-op si no hay Steam).
4. **`feat/fork-ipc-skeleton`** — H1, H2, H10, H12, `window.hydraFork`, tarjeta de ajustes con toggles. *DoD*: typecheck/lint OK; Hydra sin Steam arranca sin errores.
5. **`feat/steam-input-shortcuts`** — modo `steam-shortcut`, H3, H9. *DoD*: juego custom obtiene shortcut tras reinicio confirmado; Jugar lanza por rungameid; se cuenta playtime; fallback sin Steam.
6. **`feat/steam-input-templates`** — catálogo + caché + aplicar (URI W3 como MVP; configset W4 tras flag experimental). *DoD*: búsqueda por AppID devuelve plantillas (o fallback manual); aplicar deja el layout activo (checklist Windows).
7. **`feat/sisr-sessions`** — modo `sisr-nosteam`, H4, H5, H6. *DoD*: arranca solo bajo demanda con flag; solo se mata nuestra instancia (tests con procesos falsos); SISR externo se adopta; sin huérfanos al cerrar Hydra.
8. **`feat/sisr-steam-layouts`** — modo `sisr-steam` (shortcut `SISR - <Juego>`, `--config` en `LaunchOptions`). *DoD*: layout por juego aplicado; primera sesión sin reinicio cae a SISR directo.
9. **`feat/library-providers-core`** — interfaz, registro, dedupe, adaptador Steam, H11, `ForkImportModal`. *DoD*: tests de dedupe (mayúsculas, symlinks, appid Steam) verdes; importar dos veces crea 0 duplicados.
10. **`feat/library-provider-epic`** — *DoD*: fixtures de `.item` reales; lanzamiento por exe o URI.
11. **`feat/library-provider-gog`** — *DoD*: registro (mock) + fixtures `.info`.
12. **`feat/linux-platform-adapters`** — Flatpak Steam, Heroic, AppImage SISR. *DoD*: tests con rutas Linux; checklist manual en Bazzite.

### Anexo: reproducción de spikes ejecutados
- P1: `cd spikes/vdf-codec && SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk PATH=~/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH cargo run`
- P2: `node spikes/shortcut-id/check.mjs`; comparación Node: `node spikes/shortcut-id/sse-roundtrip.cjs <ruta steam-shortcut-editor> spikes/fixtures/sparse-indexes.vdf`
- El `shortcuts.vdf` real del usuario (13 B, vacío) solo se leyó; SHA-256 verificado antes/después.
