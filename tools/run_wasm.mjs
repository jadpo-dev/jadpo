// Runs a Rust program built for wasm32-unknown-unknown under Node, which
// has WebAssembly built in: it calls the module's exported `main` and
// exits with its code, or 1 if it traps (a panic aborts). Used by the gate
// (tools/verify.py) to run the Decimal maths' reference check
// (crates/jadpo-value/examples/decimal_maths) on WebAssembly.
//
//   node tools/run_wasm.mjs target/wasm32-unknown-unknown/debug/examples/decimal_maths.wasm

import { readFileSync } from 'node:fs';

const path = process.argv[2];
if (!path) {
  console.error('usage: node tools/run_wasm.mjs <program.wasm>');
  process.exit(2);
}
const { instance } = await WebAssembly.instantiate(readFileSync(path), {});
try {
  const code = instance.exports.main(0, 0);
  console.log(code === 0 ? `${path}: passed` : `${path}: ${code} failed`);
  process.exit(code);
} catch (error) {
  console.error(`${path}: trapped: ${error.message}`);
  process.exit(1);
}
