// The async write() path in node:zlib (zlib/brotli/zstd) caches raw pointers
// into the JS-owned input, output, and _writeState ArrayBuffers, then returns
// to JS while the work runs on the threadpool. Calling buffer.transfer(0) in
// that window used to free the backing store out from under the native code
// (heap-use-after-free under ASAN; detached buffers / corrupt output in
// release builds).
//
// After the fix, those ArrayBuffers are pinned while native code holds a
// pointer into them (in/out per write, _writeState for the stream's
// lifetime), so transfer() takes a copy path and the stream still produces
// correct output.

import { describe, expect, it } from "bun:test";
import { bunEnv, bunExe } from "harness";
import * as zlib from "node:zlib";

describe.concurrent.each([
  ["Inflate", "deflateSync", "createInflate"],
  ["BrotliDecompress", "brotliCompressSync", "createBrotliDecompress"],
  ["ZstdDecompress", "zstdCompressSync", "createZstdDecompress"],
])("async %s with ArrayBuffer.transfer()", (name, compressSync, createDecompress) => {
  const SIZE = 65536;

  it("round-trips 64KB", async () => {
    const input = Buffer.alloc(SIZE, 0x41);
    const compressed = zlib[compressSync](input);
    const s = zlib[createDecompress]({ chunkSize: SIZE });
    const chunks = [];
    const { promise, resolve, reject } = Promise.withResolvers();
    s.on("data", c => chunks.push(c));
    s.on("end", resolve);
    s.on("error", reject);
    s.end(compressed);
    await promise;
    expect(Buffer.concat(chunks).equals(input)).toBe(true);
  });

  // One variant per buffer the native handle holds a raw pointer into during
  // an async write: the input chunk, the output buffer, and the shared
  // `_writeState` Uint32Array. Random input keeps `compressed` large enough to
  // own its ArrayBuffer (not the Buffer pool), so the input variant transfers
  // exactly the buffer the worker reads from.
  for (const [label, transferExpr] of [
    ["the input chunk's buffer", "compressed.buffer"],
    ["_outBuffer.buffer", "s._outBuffer.buffer"],
    ["_writeState.buffer", "s._writeState.buffer"],
  ]) {
    it(`survives ${label}.transfer(0) mid-write`, async () => {
      // Spawned so a pre-fix UAF surfaces as a non-zero exit instead of
      // crashing the test runner. Positive post-condition only (no stderr grep).
      const script = /* js */ `
        const z = require("zlib");
        const input = require("crypto").randomBytes(${SIZE});
        const compressed = z.${compressSync}(input);
        const s = z.${createDecompress}({ chunkSize: ${SIZE} });
        const chunks = [];
        s.on("data", c => chunks.push(c));
        s.on("end", () => {
          const out = Buffer.concat(chunks);
          if (out.equals(input)) console.log("OK");
          else console.log("BAD len=" + out.length);
        });
        s.on("error", e => console.log("ERR " + e.message));
        s.write(compressed);
        ${transferExpr}.transfer(0);
        s.end();
      `;
      await using proc = Bun.spawn({
        cmd: [bunExe(), "-e", script],
        env: bunEnv,
        stderr: "pipe",
      });
      const [stdout, stderr, exitCode] = await Promise.all([proc.stdout.text(), proc.stderr.text(), proc.exited]);
      expect(stdout.trim()).toBe("OK");
      expect(exitCode).toBe(0);
    });
  }
});
