# JWT dependency and independent boundary evidence

AUTH-001 section 2 and AUTH-P6 require an explicit JWT capability, one exactly
pinned compiler-selected package with no transitive runtime packages, real
signature validation, bounded provider/key loading, no provider claims in
business code, and evidence beyond hand-picked happy paths. The package must
not appear in non-JWT applications. This report covers the dependency,
standalone verifier and upstream evidence; generated application integration
and non-JWT artifact checks are owned by the parent package.

## Pin and provenance

The selected release is `jose@6.2.12`, MIT, from the official npm registry.
The canonical compiler manifest, Bun 1.2.20 lock and dependency evidence are
under `jadpo/crates/core/src/runtime/jwt/`. Its SHA-512 integrity is:

```
sha512-9NiFmJEex0sy2Dk58j2UGBSHgUs2ypF9eZSu4L6vjOX3Dp96Sw1F3uL+H+D1sx02jZZdzUT0HgvCy59CuvXcWw==
```

Official package metadata and the verified package's own `package.json` have
no dependency, optional-dependency or peer-dependency entries. Explicit frozen
Bun installation installed one package; exact archive comparison subsequently
verified **all 80 installed files**. Both the existing cache and a fresh
disposable installation passed exact-file verification; the fresh-install log
is `build/validation/jwt-dependency-evidence/explicit-clean-install.log`. No package manifest or dependency was added
to the repository root or a non-JWT application.

The source tag `v6.2.12` points to commit
`505a55b8f73536082367b2614cb77e927ba96ec1`. GitHub reports that annotated tag's
signature verification as false; this is not represented as a signed tag.
Package provenance was separately verified using npm 11.16.0:

```sh
npm ci --prefix build/validation/jwt-signature-probe --ignore-scripts --no-fund --no-audit
npm audit signatures --prefix build/validation/jwt-signature-probe
npm audit signatures --prefix build/validation/jwt-signature-probe --json --include-attestations
```

Result: **one verified registry signature, one verified attestation**, with
empty `invalid` and `missing` arrays. The npm lock's integrity equals the
compiler's Bun lock. This verifies publisher/provenance assertions, not code
correctness or freedom from vulnerabilities. The preceding lock-only attempt
reported no installed dependencies to audit; verification was rerun against the
actual installed package, rather than counting that attempt as evidence.

The official upstream advisory API listed six published advisories. None of
their affected ranges includes 6.2.12. This is a dated known-advisory review,
not a claim that no vulnerability exists. Sources: [release](https://github.com/panva/jose/releases/tag/v6.2.12),
[registry metadata](https://registry.npmjs.org/jose/6.2.12),
[upstream advisories](https://github.com/panva/jose/security/advisories),
[security model](https://github.com/panva/jose/security).

Raw metadata, verified archive, attestation bundles, npm verification JSON/log,
GitHub tag/commit records and advisory responses are retained under ignored
`build/validation/jwt-dependency-evidence/`. Durable pin/digest/verification
summaries live in the compiler's `jwt/dependency.json`.

## Installer hardening and offline checks

`python3 tools/install-jwt-dependency.py` is an explicit installation step. It
reads the canonical compiler files and invokes frozen Bun installation with
lifecycle scripts disabled. It does not run automatically on module import.

A frozen lock alone does not prove that existing `node_modules` files remain
unaltered. The helper now validates the pinned tarball's SHA-512, enforces
archive-size/unpacked-size/file-count limits, rejects unsafe paths, duplicates,
links and special entries, and compares the exact installed file inventory and
bytes against the verified archive. Modified existing installations are
rejected before Bun runs; missing/extra/substituted files are also rejected
after installation. Symlinked package roots, installed directories/files and
cache archives are rejected. Downloads are HTTPS-only with curl byte/time
bounds; a concurrently created cache is not overwritten.

```sh
python3 -m unittest discover -s tests/validation -p test_jwt_dependency.py -v
```

**12 offline unit tests passed**, using synthetic tarballs and disposable
filesystem trees. They cover accepted baseline inventory, archive digest
substitution, installed code substitution despite unchanged package metadata,
missing/extra files, file/directory/root symlinks, traversal, duplicate archive
members, hardlinks, archive resource bounds, rejection before any Bun process,
unsafe cache archives, and tampered upstream archive/source trees. No unit test
uses network access or an online package installation.

Focused logs: `/private/tmp/jadpo-jwt-dependency-unit.log` and
`build/validation/jwt-dependency-evidence/explicit-install.log`.

## Real cryptography and hostile input

Before handing the runtime to the integration owner,
`tests/runtime/jwt-authentication.test.ts` passed **18 cases / 3,239 assertions**
under Bun 1.2.20 with auto-install disabled and `/dev/null` as the env file.
It verifies real RS256 and ES256 signatures through the pinned `jose` package;
only a frozen subject/strength result crosses the verifier boundary. Forged
local user IDs, roles and email claims do not become application identity.

Negative cases cover missing/wrong claims, malformed audience arrays,
issuer/audience mismatch, expiry/not-before/future issuance and fixed skew,
signature substitution, `none`/HMAC confusion, attacker key URLs/embedded keys,
critical headers, duplicate escaped JSON keys, invalid UTF-8, deep nesting,
malformed/oversized/noncanonical tokens, duplicate key IDs/private key material,
key-count/key-size bounds, discovery mismatch, invalid HTTPS settings,
coalesced key refresh, cooldown, stale-cache outage and timeout abortion.
Startup exposes a closed `configured` flag; provider discovery/JWKS availability
remains a bounded request-time dependency, not a proven startup readiness check.

The deterministic fuzz corpus contains 2,048 malformed compact-token strings
(seed `0x12345678`) and 1,024 single-character corruptions of a genuinely signed
structured JWT (seed `0x76543210`). This is reproducible parser/verification
pressure, not an exhaustive fuzzer or cryptographic proof. Raw focused output:
`/private/tmp/jadpo-jwt-runtime.log`.

## Upstream conformance scope

The original upstream `tap/jws.ts`, `tap/jwk.ts` and `tap/cookbook.ts` modules
were loaded unchanged against the verified **released npm implementation**.
The broad run produced **79 passing / 15 failing cases out of 94**. All 15
failures concern ML-DSA primitives unavailable in Bun 1.2.20; those algorithms
are outside Jadpo's fixed RS256/ES256 allowlist. The failing broad log is retained
as `upstream-conformance.log`, rather than being relabelled a passing full suite.

An explicitly filtered run of the original tests matching `RS256|ES256` passed
**8 cases / 45 assertions, zero skips**. This establishes only that selected
upstream signature/JWT and public/private JWK import cases pass on this host.
It is not the full upstream suite, all JOSE standards, or deployment assurance.

```sh
python3 tools/verify-jwt-upstream.py --prepare
python3 tools/verify-jwt-upstream.py
# Optional broad run deliberately exposes the unsupported ML-DSA failures:
python3 tools/verify-jwt-upstream.py --broad
```

Preparation explicitly downloads the digest-pinned upstream archive and uses a
frozen lock for test-only QUnit 2.26.0 and its five tooling dependencies. These
six tooling packages are isolated under `build/validation/jwt-upstream-harness`;
they are not generated application/runtime dependencies. Every run revalidates
the archive digest, compares every extracted source file against the archive,
checks the canonical harness manifest/lock, and verifies every installed jose
runtime file. The QUnit tooling installation itself has a frozen lock but does
not yet receive the same installed-file comparison as the shipped jose package.

Latest reproduction log:
`build/validation/jwt-dependency-evidence/upstream-allowlist-reproduced.log`.
No full workspace gate was run by this worker during the final tool-hardening
package. No claim of complete AUTH-P6 integration or release readiness follows
from these bounded checks.
