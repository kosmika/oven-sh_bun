import { install_test_helpers } from "bun:internal-for-testing";
import { expect, test } from "bun:test";
import { bunEnv, bunExe, tempDir } from "harness";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const { parseLockfile } = install_test_helpers;

// These tests exercise the raw-byte validation loops in the binary-lockfile
// loader (`Package::load_fields`), which iterate the `meta`/`bin` columns and
// reject out-of-range enum discriminants before the bytes are reinterpreted as
// `#[repr(u8)]` enums. A `file:` tarball dependency is used so the lockfile is
// produced and re-parsed entirely offline — no registry needed. `parseLockfile`
// drives `Lockfile::load_from_dir`, which runs `load_fields`.

const tarball = join(import.meta.dir, "bar-0.0.2.tgz");

async function installFileDep(dir: string) {
  copyFileSync(tarball, join(dir, "bar-0.0.2.tgz"));
  await using proc = Bun.spawn({
    cmd: [bunExe(), "install", "--no-progress"],
    cwd: dir,
    env: bunEnv,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stderr, exitCode] = await Promise.all([proc.stderr.text(), proc.exited]);
  expect(stderr).not.toContain("error:");
  expect(exitCode).toBe(0);
}

// Locate the `meta` column in a binary lockfile. Packages are stored SoA; the
// `meta` column follows name (8), name_hash (8), resolution (72 in format v3,
// 64 in v2), dependencies (8) and resolutions (8) per package. Each Meta record
// is 88 bytes.
function metaColumn(lockb: Buffer) {
  const fmt = lockb.readUInt32LE(42);
  const n = Number(lockb.readBigUInt64LE(86));
  const begin = Number(lockb.readBigUInt64LE(110));
  const resolutionSize = fmt === 2 ? 64 : 72;
  const metaStart = begin + n * (8 + 8 + resolutionSize + 8 + 8);
  return { n, metaStart };
}

test("valid binary lockfile round-trips through the field loader", async () => {
  using dir = tempDir("lockb-field-valid", {
    "package.json": JSON.stringify({
      name: "lockb-field-valid",
      version: "1.0.0",
      dependencies: { "dummy-package": "file:./bar-0.0.2.tgz" },
    }),
    "bunfig.toml": "[install]\nsaveTextLockfile = false\n",
  });
  await installFileDep(String(dir));

  const parsed = parseLockfile(String(dir)) as { packages?: Record<string, unknown> };
  // Loading succeeds, which means `load_fields` ran its meta/bin validation
  // loops over the real column bytes without rejecting them.
  expect(parsed.packages).toBeDefined();
  expect(Object.keys(parsed.packages!).length).toBe(2);
});

test("rejects a binary lockfile whose meta.origin byte is out of range", async () => {
  using dir = tempDir("lockb-field-origin", {
    "package.json": JSON.stringify({
      name: "lockb-field-origin",
      version: "1.0.0",
      dependencies: { "dummy-package": "file:./bar-0.0.2.tgz" },
    }),
    "bunfig.toml": "[install]\nsaveTextLockfile = false\n",
  });
  await installFileDep(String(dir));

  const lockbPath = join(String(dir), "bun.lockb");
  const lockb = readFileSync(lockbPath);
  const { n, metaStart } = metaColumn(lockb);

  // `Meta.origin` is the first byte of each 88-byte record; the `Origin` enum
  // is `#[repr(u8)]` with discriminants 0..=2, so 0x42 is out of range and the
  // per-element check in the `meta` validation loop must reject it.
  expect(n).toBeGreaterThan(0);
  const originOffset = metaStart + (n - 1) * 88 + 0;
  expect(lockb[originOffset]).toBeLessThanOrEqual(2); // sanity: valid before
  lockb[originOffset] = 0x42;
  writeFileSync(lockbPath, lockb);

  expect(() => parseLockfile(String(dir))).toThrow("Lockfile validation failed: invalid package meta");
});
