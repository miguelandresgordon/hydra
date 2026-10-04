import { NativeAddon } from "@main/services/native-addon";

import { SafeShortcutsStore } from "./safe-shortcuts-store";

/** Process-wide store backed by the lossless Rust codec (`hydra-native`). */
export const forkShortcutsStore = new SafeShortcutsStore({
  read: (filePath) => NativeAddon.steamShortcutsRead(filePath),
  write: (filePath, treeJson) =>
    NativeAddon.steamShortcutsWrite(filePath, treeJson),
});
