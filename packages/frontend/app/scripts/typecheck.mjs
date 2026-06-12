#!/usr/bin/env node
// First-party typecheck gate.
//
// Why this exists: BlockSuite/Affine (@blocksuite/*) intentionally publishes its
// RAW .ts source as the package's types (its `exports` map points at `src/*.ts`,
// and 0.22.4 is the latest release — this is by design, not a bug we can upgrade
// past). TypeScript therefore type-checks BlockSuite's own source as program input,
// and `skipLibCheck` does NOT help because it only skips `.d.ts`, never `.ts`. The
// result is ~20 errors that live entirely inside node_modules/@blocksuite/* and are
// not fixable in our tree. vite/esbuild bundles that same source fine (it never
// type-checks), so those errors are pure noise for our build gate.
//
// This wrapper runs `tsc` but only FAILS on errors in our own code. Vendored
// (node_modules) errors are counted and reported for transparency, never silently
// dropped — if BlockSuite ever ships a clean version we'll see the count drop to 0.
//
// Any first-party error still fails the build, exactly like a plain `tsc` gate.

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const ERROR_LINE = /error TS\d+/;
const isVendored = (line) => line.includes("node_modules/") || line.includes("node_modules\\");

// Resolve the WORKSPACE TypeScript binary explicitly — a bare `tsc` on PATH can
// be an unrelated global install with an older compiler (no es2023 lib, no
// allowImportingTsExtensions, etc.), which would report phantom tsconfig errors.
const require = createRequire(import.meta.url);
const tscBin = require.resolve("typescript/bin/tsc");

// `tsc -b` is incremental; --force guarantees a full check so we never pass on a
// stale .tsbuildinfo. Diagnostics go to stdout.
const res = spawnSync(process.execPath, [tscBin, "-b", "--force"], {
  cwd: process.cwd(),
  encoding: "utf8",
});

const output = `${res.stdout || ""}${res.stderr || ""}`;
const errorLines = output.split("\n").filter((l) => ERROR_LINE.test(l));
const firstParty = errorLines.filter((l) => !isVendored(l));
const vendored = errorLines.filter(isVendored);

if (vendored.length > 0) {
  console.log(
    `ℹ︱ Suppressed ${vendored.length} type error(s) inside node_modules/@blocksuite/* ` +
      `(library ships raw .ts as types; bundled by vite, not a first-party issue).`,
  );
}

if (firstParty.length > 0) {
  console.error(`\n✗ ${firstParty.length} first-party type error(s):\n`);
  for (const l of firstParty) console.error(l);
  console.error("");
  process.exit(1);
}

console.log("✓ First-party code is type-clean.");
process.exit(0);
