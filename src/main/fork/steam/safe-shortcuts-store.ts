import fs from "node:fs";
import path from "node:path";
import { isDeepStrictEqual } from "node:util";

import type { SteamShortcut } from "@types";

/** Mirror of `steam_vdf::Value` / `Entry` as serialized by `hydra-native`. */
export type VdfValue =
  | { type: "map"; value: VdfEntry[] }
  | { type: "str"; value: string }
  | { type: "u32"; value: number };

export interface VdfEntry {
  key: string;
  value: VdfValue;
}

/** Lossless codec over a `shortcuts.vdf` path (trees as JSON strings). */
export interface ShortcutsCodec {
  read(filePath: string): string;
  write(filePath: string, treeJson: string): void;
}

export interface SafeShortcutsStoreOptions {
  maxBackups?: number;
  now?: () => number;
}

const DEFAULT_MAX_BACKUPS = 5;
const SHORTCUTS_KEY = "shortcuts";

const emptyTree = (): VdfEntry[] => [
  { key: SHORTCUTS_KEY, value: { type: "map", value: [] } },
];

const toU32 = (value: number) => value >>> 0;

const toVdfValue = (value: unknown): VdfValue | null => {
  if (typeof value === "string") return { type: "str", value };
  if (typeof value === "boolean") return { type: "u32", value: value ? 1 : 0 };
  if (typeof value === "number" && Number.isFinite(value)) {
    return { type: "u32", value: toU32(value) };
  }
  return null;
};

const findShortcutsMap = (tree: VdfEntry[]): VdfEntry[] | null => {
  for (const entry of tree) {
    if (
      entry.key.toLowerCase() === SHORTCUTS_KEY &&
      entry.value.type === "map"
    ) {
      return entry.value.value;
    }
  }
  return null;
};

const entryAppId = (entry: VdfEntry): number | null => {
  if (entry.value.type !== "map") return null;
  const appId = entry.value.value.find(
    (field) => field.key.toLowerCase() === "appid"
  );
  return appId?.value.type === "u32" ? toU32(appId.value.value) : null;
};

/**
 * Overwrites the fields of an existing shortcut entry with the ones given,
 * keeping the original key spelling and every field we don't know about
 * (`tags`, fields added by newer Steam versions...).
 */
const mergeShortcutFields = (
  existing: VdfEntry[],
  shortcut: SteamShortcut
): VdfEntry[] => {
  const merged = [...existing];

  for (const [key, raw] of Object.entries(shortcut)) {
    const value = toVdfValue(raw);
    if (!value) continue;

    const index = merged.findIndex(
      (field) => field.key.toLowerCase() === key.toLowerCase()
    );
    if (index === -1) merged.push({ key, value });
    else merged[index] = { key: merged[index].key, value };
  }

  return merged;
};

/**
 * Builds the new `shortcuts` map for `shortcuts`, reusing the existing entry
 * (same appid) so unknown fields and key spelling survive. Entries are
 * renumbered densely ("0", "1", ...) like Steam and the upstream editor do:
 * its parser turns sparse indexes into arrays with holes.
 */
export const mergeShortcuts = (
  tree: VdfEntry[],
  shortcuts: SteamShortcut[]
): VdfEntry[] => {
  const current = findShortcutsMap(tree) ?? [];
  const byAppId = new Map<number, VdfEntry>();
  for (const entry of current) {
    const appId = entryAppId(entry);
    if (appId !== null && !byAppId.has(appId)) byAppId.set(appId, entry);
  }

  const next: VdfEntry[] = [];
  for (const shortcut of shortcuts) {
    const match = byAppId.get(toU32(shortcut.appid));
    const fields = match?.value.type === "map" ? match.value.value : [];
    next.push({
      key: String(next.length),
      value: { type: "map", value: mergeShortcutFields(fields, shortcut) },
    });
  }

  const others = tree.filter(
    (entry) =>
      !(entry.key.toLowerCase() === SHORTCUTS_KEY && entry.value.type === "map")
  );
  const original = tree.find(
    (entry) =>
      entry.key.toLowerCase() === SHORTCUTS_KEY && entry.value.type === "map"
  );

  return [
    {
      key: original?.key ?? SHORTCUTS_KEY,
      value: { type: "map", value: next },
    },
    ...others,
  ];
};

/**
 * Writes `shortcuts.vdf` defensively: backup, codec write (atomic, validated
 * by the codec), reparse-and-compare, and restore of the original on failure.
 */
export class SafeShortcutsStore {
  private readonly maxBackups: number;
  private readonly now: () => number;
  private readonly queues = new Map<string, Promise<unknown>>();

  constructor(
    private readonly codec: ShortcutsCodec,
    options: SafeShortcutsStoreOptions = {}
  ) {
    this.maxBackups = options.maxBackups ?? DEFAULT_MAX_BACKUPS;
    this.now = options.now ?? Date.now;
  }

  /** Reads the tree; a missing file is an empty `shortcuts` map. */
  public read(filePath: string): VdfEntry[] {
    if (!fs.existsSync(filePath)) return emptyTree();
    return JSON.parse(this.codec.read(filePath)) as VdfEntry[];
  }

  /** Replaces the shortcuts of the file with `shortcuts`, losslessly. */
  public writeShortcuts(filePath: string, shortcuts: SteamShortcut[]) {
    return this.serialized(filePath, () => {
      const tree = mergeShortcuts(this.read(filePath), shortcuts);
      this.writeTreeUnsafe(filePath, tree);
    });
  }

  public writeTree(filePath: string, tree: VdfEntry[]) {
    return this.serialized(filePath, () =>
      this.writeTreeUnsafe(filePath, tree)
    );
  }

  public listBackups(filePath: string): string[] {
    const dir = path.dirname(filePath);
    const pattern = this.backupPattern(filePath);

    let names: string[];
    try {
      names = fs.readdirSync(dir);
    } catch {
      return [];
    }

    return names
      .filter((name) => pattern.test(name))
      .sort()
      .map((name) => path.join(dir, name));
  }

  private serialized<T>(filePath: string, task: () => T): Promise<T> {
    const previous = this.queues.get(filePath) ?? Promise.resolve();
    const run = previous.then(task, task);
    const tail = run.catch(() => undefined);
    this.queues.set(filePath, tail);
    void tail.then(() => {
      if (this.queues.get(filePath) === tail) this.queues.delete(filePath);
    });
    return run;
  }

  private writeTreeUnsafe(filePath: string, tree: VdfEntry[]) {
    const original = fs.existsSync(filePath) ? fs.readFileSync(filePath) : null;
    if (original) this.createBackup(filePath, original);

    try {
      this.codec.write(filePath, JSON.stringify(tree));
      const reparsed = JSON.parse(this.codec.read(filePath)) as VdfEntry[];
      if (!isDeepStrictEqual(reparsed, tree)) {
        throw new Error("reparsed shortcuts differ from the written tree");
      }
    } catch (error) {
      const restored = this.restore(filePath, original);
      const reason = error instanceof Error ? error.message : String(error);
      throw new Error(
        `Could not write ${filePath}: ${reason} (${
          restored ? "original restored" : "RESTORE FAILED"
        })`,
        { cause: error }
      );
    }

    this.pruneBackups(filePath);
  }

  private createBackup(filePath: string, content: Buffer) {
    const base = this.now();
    const existing = new Set(this.listBackups(filePath));

    // Same-millisecond writes must not overwrite each other's backup.
    for (let stamp = base; ; stamp += 1) {
      const backupPath = this.backupPath(filePath, stamp);
      if (existing.has(backupPath)) continue;
      fs.writeFileSync(backupPath, content, { flag: "wx" });
      return;
    }
  }

  private restore(filePath: string, original: Buffer | null): boolean {
    try {
      if (!original) {
        fs.rmSync(filePath, { force: true });
        return true;
      }

      const tmp = `${filePath}.hydra.restore`;
      fs.writeFileSync(tmp, original);
      fs.renameSync(tmp, filePath);
      return true;
    } catch {
      return false;
    }
  }

  private pruneBackups(filePath: string) {
    const backups = this.listBackups(filePath);
    const excess = backups.length - this.maxBackups;
    for (const stale of backups.slice(0, Math.max(excess, 0))) {
      fs.rmSync(stale, { force: true });
    }
  }

  // Zero-padded so lexicographic order is chronological.
  private backupPath(filePath: string, stamp: number) {
    return `${filePath}.hydra.${String(stamp).padStart(15, "0")}.bak`;
  }

  private backupPattern(filePath: string) {
    const escaped = path
      .basename(filePath)
      .replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`^${escaped}\\.hydra\\.\\d+\\.bak$`);
  }
}
