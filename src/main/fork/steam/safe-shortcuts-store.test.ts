import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";

import { parseBuffer, writeBuffer } from "steam-shortcut-editor";

import type { SteamShortcut } from "@types";

import {
  SafeShortcutsStore,
  type ShortcutsCodec,
  type VdfEntry,
  type VdfValue,
} from "./safe-shortcuts-store.js";

// Minimal binary VDF codec standing in for the Rust addon (same JSON shape).
const TAG_MAP = 0;
const TAG_STR = 1;
const TAG_U32 = 2;
const TAG_END = 8;

const encodeMap = (entries: VdfEntry[], out: number[]) => {
  const cstr = (s: string) => out.push(...Buffer.from(s, "utf8"), 0);
  for (const { key, value } of entries) {
    if (value.type === "map") {
      out.push(TAG_MAP);
      cstr(key);
      encodeMap(value.value, out);
    } else if (value.type === "str") {
      out.push(TAG_STR);
      cstr(key);
      cstr(value.value);
    } else {
      out.push(TAG_U32);
      cstr(key);
      const buf = Buffer.alloc(4);
      buf.writeUInt32LE(value.value);
      out.push(...buf);
    }
  }
  out.push(TAG_END);
};

const decodeMap = (buf: Buffer, pos: { i: number }): VdfEntry[] => {
  const cstr = () => {
    const end = buf.indexOf(0, pos.i);
    assert.notEqual(end, -1, "unterminated string");
    const s = buf.toString("utf8", pos.i, end);
    pos.i = end + 1;
    return s;
  };
  const entries: VdfEntry[] = [];
  for (;;) {
    const tag = buf[pos.i++];
    if (tag === TAG_END) return entries;
    const key = cstr();
    let value: VdfValue;
    if (tag === TAG_MAP) value = { type: "map", value: decodeMap(buf, pos) };
    else if (tag === TAG_STR) value = { type: "str", value: cstr() };
    else if (tag === TAG_U32) {
      value = { type: "u32", value: buf.readUInt32LE(pos.i) };
      pos.i += 4;
    } else throw new Error(`unknown tag ${tag}`);
    entries.push({ key, value });
  }
};

const fakeCodec: ShortcutsCodec = {
  read: (filePath) =>
    JSON.stringify(decodeMap(fs.readFileSync(filePath), { i: 0 })),
  write: (filePath, treeJson) => {
    const out: number[] = [];
    encodeMap(JSON.parse(treeJson) as VdfEntry[], out);
    fs.writeFileSync(filePath, Buffer.from(out));
  },
};

const shortcut = (appid: number, appname: string): SteamShortcut => ({
  appid,
  appname,
  Exe: `"C:\\Games\\${appname}.exe"`,
  StartDir: `"C:\\Games"`,
  icon: "",
  ShortcutPath: "",
  LaunchOptions: "",
  IsHidden: false,
  AllowDesktopConfig: true,
  AllowOverlay: true,
  OpenVR: false,
  Devkit: false,
  DevkitGameID: "",
  DevkitOverrideAppID: false,
  LastPlayTime: 0,
  FlatpakAppID: "",
});

const str = (value: string): VdfValue => ({ type: "str", value });
const u32 = (value: number): VdfValue => ({ type: "u32", value });

describe("SafeShortcutsStore", () => {
  let dir: string;
  let file: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "hydra-shortcuts-"));
    file = path.join(dir, "shortcuts.vdf");
  });

  afterEach(() => {
    fs.rmSync(dir, { recursive: true, force: true });
  });

  it("creates the file when it does not exist, without a backup", async () => {
    const store = new SafeShortcutsStore(fakeCodec);

    await store.writeShortcuts(file, [shortcut(0x80000001, "Game A")]);

    const parsed = parseBuffer(fs.readFileSync(file));
    assert.equal(parsed.shortcuts.length, 1);
    assert.equal(parsed.shortcuts[0].appname, "Game A");
    assert.deepEqual(store.listBackups(file), []);
  });

  it("writes files the upstream parser reads back (create/delete flow)", async () => {
    const store = new SafeShortcutsStore(fakeCodec);
    fs.writeFileSync(
      file,
      writeBuffer({ shortcuts: [shortcut(0x80000001, "Game A")] })
    );

    const existing = parseBuffer(fs.readFileSync(file))
      .shortcuts as SteamShortcut[];
    existing.push(shortcut(0x80000002, "Game B"));
    await store.writeShortcuts(file, existing);

    let parsed = parseBuffer(fs.readFileSync(file)).shortcuts;
    assert.deepEqual(
      parsed.map((s: SteamShortcut) => [s.appid, s.appname, s.AllowOverlay]),
      [
        [0x80000001, "Game A", true],
        [0x80000002, "Game B", true],
      ]
    );

    await store.writeShortcuts(file, [existing[1]]);
    parsed = parseBuffer(fs.readFileSync(file)).shortcuts;
    assert.deepEqual(
      parsed.map((s: SteamShortcut) => s.appname),
      ["Game B"]
    );
  });

  it("preserves unknown fields and key spelling, renumbering densely", async () => {
    const store = new SafeShortcutsStore(fakeCodec);
    const original: VdfEntry[] = [
      {
        key: "shortcuts",
        value: {
          type: "map",
          value: [
            {
              key: "7",
              value: {
                type: "map",
                value: [
                  { key: "appid", value: u32(0x80000001) },
                  { key: "AppName", value: str("Old name") },
                  {
                    key: "tags",
                    value: {
                      type: "map",
                      value: [{ key: "0", value: str("Favorite") }],
                    },
                  },
                  { key: "FutureField", value: u32(42) },
                ],
              },
            },
          ],
        },
      },
    ];
    await store.writeTree(file, original);
    await store.writeShortcuts(file, [
      shortcut(0x80000001, "New name"),
      shortcut(0x80000002, "Game B"),
    ]);

    const tree = store.read(file);
    const entries = (tree[0].value as { value: VdfEntry[] }).value;
    assert.deepEqual(
      entries.map((e) => e.key),
      ["0", "1"]
    );
    const fields = (entries[0].value as { value: VdfEntry[] }).value;
    const byKey = Object.fromEntries(fields.map((f) => [f.key, f.value]));
    assert.deepEqual(byKey.AppName, str("New name"));
    assert.equal("appname" in byKey, false);
    assert.deepEqual(byKey.FutureField, u32(42));
    assert.equal(byKey.tags.type, "map");
  });

  it("restores the original when the write leaves corrupt bytes", async () => {
    const store = new SafeShortcutsStore(fakeCodec);
    await store.writeShortcuts(file, [shortcut(0x80000001, "Game A")]);
    const before = fs.readFileSync(file);

    const corrupting = new SafeShortcutsStore({
      read: fakeCodec.read,
      write: (filePath) => fs.writeFileSync(filePath, Buffer.from([1, 2, 3])),
    });

    await assert.rejects(
      corrupting.writeShortcuts(file, [shortcut(0x80000002, "Game B")]),
      /original restored/
    );
    assert.deepEqual(fs.readFileSync(file), before);
    assert.equal(fs.existsSync(`${file}.hydra.restore`), false);
  });

  it("restores the original when the codec throws mid-write", async () => {
    const store = new SafeShortcutsStore(fakeCodec);
    await store.writeShortcuts(file, [shortcut(0x80000001, "Game A")]);
    const before = fs.readFileSync(file);

    const failing = new SafeShortcutsStore({
      read: fakeCodec.read,
      write: (filePath) => {
        fs.writeFileSync(filePath, Buffer.alloc(0));
        throw new Error("disk full");
      },
    });

    await assert.rejects(
      failing.writeShortcuts(file, [shortcut(0x80000002, "Game B")]),
      /disk full.*original restored/
    );
    assert.deepEqual(fs.readFileSync(file), before);
  });

  it("removes a file it created when the first write fails", async () => {
    const failing = new SafeShortcutsStore({
      read: fakeCodec.read,
      write: (filePath) => fs.writeFileSync(filePath, Buffer.from([9])),
    });

    await assert.rejects(
      failing.writeShortcuts(file, [shortcut(0x80000001, "Game A")])
    );
    assert.equal(fs.existsSync(file), false);
  });

  it("does not write when the original cannot be parsed", async () => {
    const store = new SafeShortcutsStore(fakeCodec);
    fs.writeFileSync(file, Buffer.from([1, 2, 3]));

    await assert.rejects(
      store.writeShortcuts(file, [shortcut(0x80000001, "Game A")])
    );
    assert.deepEqual(fs.readFileSync(file), Buffer.from([1, 2, 3]));
    assert.deepEqual(store.listBackups(file), []);
  });

  it("keeps at most 5 backups, dropping the oldest", async () => {
    let clock = 1_000;
    const store = new SafeShortcutsStore(fakeCodec, { now: () => clock++ });

    for (let i = 0; i < 9; i++) {
      await store.writeShortcuts(file, [shortcut(0x80000001 + i, `Game ${i}`)]);
    }

    const backups = store.listBackups(file);
    assert.equal(backups.length, 5);
    // 9 writes -> 8 backups (first write had no file); newest 5 are kept and
    // the newest holds the state before the last write ("Game 7").
    const newest = parseBuffer(fs.readFileSync(backups[4])).shortcuts;
    assert.equal(newest[0].appname, "Game 7");
    const oldest = parseBuffer(fs.readFileSync(backups[0])).shortcuts;
    assert.equal(oldest[0].appname, "Game 3");
  });

  it("never overwrites a backup written in the same millisecond", async () => {
    const store = new SafeShortcutsStore(fakeCodec, { now: () => 5 });

    for (let i = 0; i < 3; i++) {
      await store.writeShortcuts(file, [shortcut(0x80000001 + i, `Game ${i}`)]);
    }

    assert.equal(store.listBackups(file).length, 2);
  });

  it("serializes concurrent writes to the same file", async () => {
    const store = new SafeShortcutsStore(fakeCodec);

    await Promise.all(
      [0, 1, 2, 3].map((i) =>
        store.writeShortcuts(file, [shortcut(0x80000001 + i, `Game ${i}`)])
      )
    );

    const parsed = parseBuffer(fs.readFileSync(file)).shortcuts;
    assert.equal(parsed.length, 1);
    assert.equal(parsed[0].appname, "Game 3");
  });
});
