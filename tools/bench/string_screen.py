#!/usr/bin/env python3
"""One bounded diagnostic Node-hosted Wasm writer screen, not HTTP evidence."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import ai_accounting as guard
import golden
import request_profile as profiles

ROOT = guard.ROOT
WORK = ROOT/'tools/bench/work/string-screen'
QUALIFICATION = 'diagnostic Node-hosted Wasm; not HTTP or workerd'
OLD = '190df01270b52ce4b3a7190e933091239647ea41'
BYTE = '1db7e87261685eda3de3c7efcc7a2a0b16cc12bc'
VARIANTS = ('old', 'reserve', 'byte_bulk', 'char_bulk', 'skip_empty', 'first_escape')


def canonical(text):
    # Independent standard serializer; normalize only its two alternative
    # short spellings, parsing escape pairs so literal backslashes stay literal.
    raw = json.dumps(text, ensure_ascii=False)
    result = []; index = 0
    while index < len(raw):
        if raw[index] == '\\':
            pair = raw[index:index+2]
            result.append({'\\b': '\\u0008', '\\f': '\\u000c'}.get(pair, pair))
            index += 2
        else:
            result.append(raw[index]); index += 1
    return ''.join(result).encode()


def case_specs():
    return [
        dict(name='small-ascii', fragment='x', repeats=100, tail='', iterations=200000),
        dict(name='large-ascii', fragment='x', repeats=65536, tail='', iterations=1024),
        dict(name='late-escape', fragment='x', repeats=65535, tail='\n', iterations=1024),
        dict(name='ordinary-unicode', fragment='é🙂', repeats=32768, tail='', iterations=256),
        dict(name='mixed-unicode', fragment='é🙂"\\\n\t', repeats=10922, tail='é🙂"\\', iterations=256),
        dict(name='dense-ascii', fragment='"\\\n\t\r\0\b\f', repeats=8192, tail='', iterations=128),
    ]


def case_metadata():
    result = []
    for case in case_specs():
        text = case['fragment']*case['repeats']+case['tail']
        encoded = text.encode(); expected = canonical(text)
        result.append({'name': case['name'], 'iterations': case['iterations'],
                       'scalars': len(text), 'utf8_bytes': len(encoded),
                       'input_sha256': hashlib.sha256(encoded).hexdigest(),
                       'output_bytes': len(expected),
                       'output_sha256': hashlib.sha256(expected).hexdigest()})
    return result


def extract_writer(source):
    marker = 'pub fn write_string(text: &str, out: &mut String) {'
    try:
        start = source.index(marker); end = source.index('\n#[cfg(test)]', start)
    except ValueError:
        raise ValueError('Missing or unbounded reference writer') from None
    result = source[start:end].strip()
    if not result.endswith('\n}'):
        raise ValueError('Reference writer boundary differs')
    return result


def writer_sources():
    def original(ref):
        return extract_writer(subprocess.check_output(
            ['git', 'show', ref+':crates/jadpo-value/src/json.rs'], cwd=ROOT, text=True))
    old, byte = original(OLD), original(BYTE)
    marker = 'pub fn write_string(text: &str, out: &mut String) {'
    reserve = old.replace(marker, marker+'\n    out.reserve(text.len() + 2);', 1)
    scalar = byte.replace('for (index, byte) in text.bytes().enumerate()',
                          'for (index, byte) in text.char_indices()')
    scalar = scalar.replace("byte >= 0x20", "byte >= '\\u{20}'")
    for character in ('"', '\\\\', '\\n', '\\r', '\\t'):
        scalar = scalar.replace("b'"+character+"'", "'"+character+"'")
    scalar = scalar.replace('write!(out, "\\\\u{control:04x}")',
                            'write!(out, "\\\\u{:04x}", control as u32)')
    skip = byte.replace('out.push_str(&text[start..index]);',
                         'if start != index { out.push_str(&text[start..index]); }')
    loop = old[old.index('    for c in text.chars()'):old.rindex("    out.push('\"');")]
    loop = loop.replace('text.chars()', 'text[index..].chars()', 1)
    loop = ''.join('    '+line if line.strip() else line for line in loop.splitlines(keepends=True))
    first = marker + '''
    out.reserve(text.len() + 2);
    out.push('"');
    if let Some(index) = text.bytes().position(|byte| byte < 0x20 || byte == b'"' || byte == b'\\\\') {
        out.push_str(&text[..index]);
''' + loop + '''
    } else {
        out.push_str(text);
    }
    out.push('"');
}'''
    sources = dict(zip(VARIANTS, (old, reserve, byte, scalar, skip, first)))
    return {name: source.replace('pub fn write_string', 'fn '+name, 1)
            for name, source in sources.items()}


def rust_bytes(raw):
    return '&['+','.join(map(str, raw))+']'


RUST = r'''
use std::{cell::RefCell, fmt::Write as _, hint::black_box};
type Writer = fn(&str, &mut String);
@WRITERS@
const WRITERS: [Writer; @WRITER_COUNT@] = [@WRITER_LIST@];
struct Case { fragment: &'static [u8], repeats: usize, tail: &'static [u8],
              escaped: &'static [u8], escaped_tail: &'static [u8] }
@CASES@
@FIXED@
fn string(bytes: &[u8]) -> &str { std::str::from_utf8(bytes).unwrap() }
fn input(case: usize) -> String {
    let c = &CASES[case];
    let mut text = string(c.fragment).repeat(c.repeats);
    text.push_str(string(c.tail));
    text
}
fn expected(case: usize) -> String {
    let c = &CASES[case];
    let mut out = String::from("\"");
    out.push_str(&string(c.escaped).repeat(c.repeats));
    out.push_str(string(c.escaped_tail));
    out.push('"');
    out
}
fn valid(writer: Writer) -> bool {
    for prefix in ["", "already written: é🙂 "] {
        for (text, answer) in FIXED {
            let mut out = prefix.to_owned();
            writer(string(text), &mut out);
            if out.as_bytes() != [prefix.as_bytes(), answer].concat() { return false; }
        }
        for case in 0..CASES.len() {
            let mut out = prefix.to_owned();
            writer(&input(case), &mut out);
            if out != format!("{prefix}{}", expected(case)) { return false; }
        }
    }
    true
}
fn wrong_escape(text: &str, out: &mut String) {
    let start = out.len();
    old(text, out);
    let changed = out[start..].replace("\\u0008", "\\b");
    out.truncate(start);
    out.push_str(&changed);
}
fn truncated(text: &str, out: &mut String) { old(text, out); out.pop(); }
#[no_mangle]
pub extern "C" fn verify() -> u32 {
    for writer in WRITERS { assert!(valid(writer)); }
    assert!(!valid(wrong_escape));
    assert!(!valid(truncated));
    (WRITERS.len() * 2 * (FIXED.len() + CASES.len()) + 2) as u32
}
thread_local! {
    static INPUT: RefCell<Option<(u32, String)>> = const { RefCell::new(None) };
}
#[no_mangle]
pub extern "C" fn prepare(case: u32) {
    INPUT.with(|slot| *slot.borrow_mut() = Some((case, input(case as usize))));
}
fn observed(out: &str) -> u64 {
    let b = out.as_bytes();
    (b.len() as u64) ^ (u64::from(b[0]) << 32)
        ^ (u64::from(b[b.len()/2]) << 16) ^ u64::from(b[b.len()-1])
}
#[no_mangle]
pub extern "C" fn expected_checksum(case: u32, iterations: u32) -> u64 {
    let value = observed(&expected(case as usize));
    let mut checksum = 0_u64;
    for _ in 0..iterations { checksum = checksum.rotate_left(7).wrapping_add(value); }
    checksum
}
#[no_mangle]
pub extern "C" fn screen(variant: u32, case: u32, iterations: u32) -> u64 {
    let writer = black_box(WRITERS[variant as usize]);
    INPUT.with(|slot| {
        let borrowed = slot.borrow();
        let (prepared_case, text) = borrowed.as_ref().unwrap();
        assert_eq!(*prepared_case, case);
        let mut checksum = 0_u64;
        for _ in 0..iterations {
            let mut out = String::new();
            writer(black_box(text), &mut out);
            checksum = checksum.rotate_left(7).wrapping_add(observed(black_box(&out)));
            black_box(&out);
        }
        checksum
    })
}
'''


NODE = r'''
import fs from 'node:fs';
import crypto from 'node:crypto';
import os from 'node:os';
const directory = process.argv[2];
const buildPath = directory + '/build.json';
const buildBytes = fs.readFileSync(buildPath);
const build = JSON.parse(buildBytes);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const moduleBytes = fs.readFileSync(directory + '/writers.wasm');
if (sha(moduleBytes) !== build.artifacts[directory + '/writers.wasm']) throw Error('Wasm changed');
const {instance} = await WebAssembly.instantiate(moduleBytes);
const api = instance.exports;
const verified = api.verify();
if (verified !== build.expected_verifications) throw Error('Canonical verifier count differs');
if (process.argv.includes('--verify-only')) {
  console.log(JSON.stringify({passed: true, byte_checks: verified - 2, negative_controls: 2,
                              versions: process.versions}));
  process.exit(0);
}
const result = {schemaVersion: build.schemaVersion, kind: 'string-screen', qualification: build.qualification,
  source: build.source, build_sha256: sha(buildBytes), node_versions: process.versions,
  canonical: {passed: true, byte_checks: verified - 2, negative_controls: 2},
  protocol: {rounds: 5, case_order: 'forward/reverse alternating', variant_order: 'rotated by round+case',
    input: 'prepared before each group; excluded', output: 'fresh String per iteration',
    checksum: 'O(1) length and three byte observations; full byte equality before timing'},
  started_unix: Date.now()/1000, rows: [], complete: false};
const output = directory + '/results.json';
const save = () => fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n');
try {
  for (let c = 0; c < build.cases.length; c++) {
    api.prepare(c);
    const warmup = c === 0 ? 2000 : 16;
    const expected = api.expected_checksum(c, warmup);
    for (let v = 0; v < build.variants.length; v++)
      if (api.screen(v, c, warmup) !== expected) throw Error('Warmup checksum differs');
  }
  const indices = build.cases.map((_, i) => i);
  for (let round = 0; round < 5; round++) {
    for (const c of (round % 2 ? [...indices].reverse() : indices)) {
      api.prepare(c);
      const iterations = build.cases[c].iterations;
      const expected = api.expected_checksum(c, iterations);
      for (let offset = 0; offset < build.variants.length; offset++) {
        const v = (round + c + offset) % build.variants.length;
        const utc = Date.now()/1000;
        const cpu = process.cpuUsage();
        const start = process.hrtime.bigint();
        const checksum = api.screen(v, c, iterations);
        const elapsed = Number(process.hrtime.bigint() - start) / 1e9;
        const used = process.cpuUsage(cpu);
        const end = Date.now()/1000;
        const row = {round: round+1, case: build.cases[c].name, variant: build.variants[v],
          iterations, elapsed_s: elapsed, ns_per_write: elapsed * 1e9 / iterations,
          node_cpu_ns_per_write: (used.user + used.system) * 1000 / iterations,
          started_unix: utc, ended_unix: end, checksum: checksum.toString(),
          load: os.loadavg()};
        result.rows.push(row);
        if (checksum !== expected) throw Error('Timed checksum differs');
        if (Math.abs(end - utc - elapsed) >= 1) throw Error('Clock discontinuity');
        save();
      }
    }
    console.error('  verified: writer screen round ' + (round+1));
  }
  result.complete = true;
} catch (error) {
  result.failure = String(error);
  throw error;
} finally {
  result.ended_unix = Date.now()/1000;
  save();
}
'''


def fixed_fixtures():
    controls = ''.join(map(chr, range(32)))
    controls_expected = (r'"\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\u0008\t\n'
                         r'\u000b\u000c\r\u000e\u000f\u0010\u0011\u0012\u0013\u0014'
                         r'\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f"').encode()
    return [('', b'""'), (controls, controls_expected),
             ('"\\\x7f', b'"\\"\\\\\x7f"'),
             ('\\b\\f', b'"\\\\b\\\\f"'),
             ('e\u0301🙂\u2028\u2029', '"e\u0301🙂\u2028\u2029"'.encode())]


def verification_count():
    return len(VARIANTS)*2*(len(fixed_fixtures())+len(case_specs()))+2


def generated_source():
    cases = []
    for c in case_specs():
        fragment, tail = c['fragment'].encode(), c['tail'].encode()
        cases.append('Case { fragment: '+rust_bytes(fragment)+', repeats: '+str(c['repeats'])
            +', tail: '+rust_bytes(tail)+', escaped: '+rust_bytes(canonical(c['fragment'])[1:-1])
            +', escaped_tail: '+rust_bytes(canonical(c['tail'])[1:-1])+' }')
    fixed = fixed_fixtures()
    rows = ',\n'.join('('+rust_bytes(t.encode())+','+rust_bytes(answer)+')' for t,answer in fixed)
    writers = writer_sources()
    source = RUST.replace('@WRITERS@', '\n'.join(writers.values())).replace(
        '@WRITER_COUNT@', str(len(VARIANTS))).replace(
        '@WRITER_LIST@', ', '.join(VARIANTS)).replace(
        '@CASES@', 'const CASES: &[Case] = &[\n'+',\n'.join(cases)+'\n];').replace(
        '@FIXED@', 'const FIXED: &[(&[u8], &[u8])] = &[\n'+rows+'\n];')
    return source, writers, verification_count()


def validate_build(stamp, current):
    if (stamp.get('schemaVersion') != 2 or stamp.get('kind') != 'string-screen'
            or stamp.get('qualification') != QUALIFICATION or stamp.get('complete') is not True
            or stamp.get('opt_level') != 's' or stamp.get('cases') != case_metadata()
            or stamp.get('canonical', {}).get('passed') is not True
            or stamp['canonical'].get('negative_controls') != 2
            or stamp['canonical'].get('byte_checks') != verification_count()-2
            or stamp.get('variants') != list(VARIANTS)
            or stamp.get('expected_verifications') != verification_count()
            or stamp.get('codegen', {}).get('indirect_call') is not True
            or stamp.get('codegen', {}).get('black_box_barrier') is not True):
        raise ValueError('Incomplete or mismatched diagnostic writer manifest')
    guard.validate(stamp, current)


def build(label):
    directory = WORK/label
    directory.mkdir(parents=True, exist_ok=False)
    before = guard.source()
    source, writers, count = generated_source()
    (directory/'writers.rs').write_text(source)
    (directory/'screen.mjs').write_text(NODE)
    compiler = Path(os.environ.get('RUSTC') or shutil.which('rustc')).absolute()
    node = Path(profiles.spike.tool('node')).resolve()
    resolved = subprocess.check_output(['rustup', 'which', 'rustc'], text=True).strip()
    command = [str(compiler), str(directory/'writers.rs'), '--crate-type=cdylib',
               '--target=wasm32-unknown-unknown', '--edition=2021', '-Copt-level=s',
               '-Clto=fat', '-Ccodegen-units=1', '-Cpanic=abort',
               '--emit=link,llvm-ir', '--out-dir', str(directory)]
    stamp = {'schemaVersion': 2, 'kind': 'string-screen', 'qualification': QUALIFICATION,
             'source': before, 'opt_level': 's', 'complete': False, 'artifacts': {},
             'variants': list(VARIANTS), 'cases': case_metadata(),
             'expected_verifications': count, 'command': command, 'node': str(node),
             'references': {'old': OLD, 'byte': BYTE},
             'writer_source_sha256': {k:hashlib.sha256(v.encode()).hexdigest() for k,v in writers.items()},
             'compiler_selection': {'requested': os.environ.get('RUSTC'), 'resolved': resolved,
                 'RUSTUP_TOOLCHAIN': os.environ.get('RUSTUP_TOOLCHAIN'),
                 'scope': 'Invoked compiler path and Rustup resolution, not a child linker trace.'},
             'rustc_version': subprocess.check_output([str(compiler), '-vV'], text=True),
             'start': profiles.context()}
    path = directory/'build.json'
    try:
        subprocess.run(command, cwd=ROOT, env={**os.environ, 'SOURCE_DATE_EPOCH': '0'}, check=True)
        for artifact in (directory/'writers.rs', directory/'writers.wasm', directory/'writers.ll',
                         directory/'screen.mjs', compiler, Path(resolved), node):
            stamp['artifacts'][str(artifact)] = golden.sha(artifact)
        # The LLVM driver must still call an opaque selected writer inside
        # its output loop. Inspect the retained IR too; this is not call counting.
        ir = (directory/'writers.ll').read_text()
        driver = ir.split('define ', 1)[-1] if '@screen(' not in ir else ir[ir.index('@screen('):]
        driver = driver.split('\n}', 1)[0]
        stamp['codegen'] = {'indirect_call': bool(re.search(r'call\s+void\s+%[\w.]+\(', driver)),
                            'black_box_barrier': 'asm sideeffect' in driver}
        if not all(stamp['codegen'].values()):
            raise ValueError('Writer driver no longer has its opaque call and optimization barrier')
        path.write_text(json.dumps(stamp, indent=2)+'\n')
        check = subprocess.check_output([str(node), str(directory/'screen.mjs'),
                                         str(directory), '--verify-only'], text=True)
        stamp['canonical'] = json.loads(check)
        stamp.update(complete=True, end=profiles.context())
        validate_build(stamp, guard.source())
    except BaseException as error:
        stamp.update(complete=False, failure=str(error), end=profiles.context())
        raise
    finally:
        path.write_text(json.dumps(stamp, indent=2)+'\n')
    print(f'passed: diagnostic writer build · {count} byte/fault checks · {directory}')


def run(label):
    directory = WORK/label
    path = directory/'build.json'
    stamp = json.loads(path.read_text())
    validate_build(stamp, guard.source())
    if (directory/'results.json').exists():
        raise ValueError('Retained writer result already exists; do not overwrite an attempt')
    try:
        subprocess.run([stamp['node'], str(directory/'screen.mjs'), str(directory)], check=True)
        validate_build(stamp, guard.source())
    except BaseException as error:
        result_path = directory/'results.json'
        if result_path.exists():
            result = json.loads(result_path.read_text())
            result.update(complete=False, post_failure=str(error))
            result_path.write_text(json.dumps(result, indent=2)+'\n')
        raise
    print(f'passed: diagnostic writer screen · {5*len(case_specs())*len(VARIANTS)} rows · {directory}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('build', 'run'))
    parser.add_argument('--label', required=True)
    args = parser.parse_args()
    if not re.fullmatch('[a-z0-9][a-z0-9-]*', args.label):
        parser.error('label must contain lower-case letters, digits and hyphens')
    print('Writer screen · '+args.command+' · diagnostic', file=sys.stderr, flush=True)
    (build if args.command == 'build' else run)(args.label)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print('failed: diagnostic writer screen\n  reason: '+str(error), file=sys.stderr)
        sys.exit(1)
