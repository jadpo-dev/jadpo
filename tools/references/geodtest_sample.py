#!/usr/bin/env python3
"""Writes the sample of GeographicLib's geodesic test set that area 20's
measures are tested against (chunk 5.5u2):
crates/jadpo-value/examples/geodesics/reference.tsv.

  python3 tools/references/geodtest_sample.py GeodTest-short.dat          # write it
  python3 tools/references/geodtest_sample.py GeodTest-short.dat --check  # fail if stale

The source is C. F. F. Karney's "Test set for geodesics" (2010),
doi:10.5281/zenodo.32156, released under Creative Commons Zero v1.0 on
Zenodo and published with GeographicLib (MIT licence) at
https://sourceforge.net/projects/geographiclib/files/testdata/GeodTest-short.dat.gz
— `GeodTest-short.dat` is every 50th of the 500,000 geodesics in
`GeodTest.dat`, 10,000 lines. Download it, `gunzip` it, and pass its path;
the script refuses any other file by its SHA-256. It isn't committed: only
the sample is, so the gate doesn't download anything and doesn't run this.

Each line of the test set is one geodesic on WGS84: lat1, lon1 (always 0)
and azi1 exact, in degrees; lat2, lon2 and azi2 to 10^-18 degrees; s12 in
metres, exact; a12 in degrees; m12, the reduced length, in metres; and S12,
the area between the geodesic and the equator, in square metres, to 1 mm².
The test set comes in nine groups, in order: 2,000 lines at random, then
1,000 each nearly antipodal, short, one end near a pole, both ends near
opposite poles, nearly meridional, nearly equatorial, running between
vertices, and ending close to a vertex.

The sample is every 20th line, from the first: 500 geodesics, 100 at random
and 50 of each other group. Each sampled line is written with its group and
its line number in the test set, leaving out azi2, and `.5` written `0.5`,
as Jadpo's decimals are. Area 20's check runs every measure on every line
(crates/jadpo-value/examples/geodesics/check.rs); every one of the 10,000
lines passes it, which a run on the whole file shows.
"""

import argparse
import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'crates/jadpo-value/examples/geodesics/reference.tsv'
SHA256 = '601de5a47f798a3c4067ca9326e8009c0acd60cd2d355b7ba3080bf114734710'
EVERY = 20
GROUPS = ([('random', 2000)] +
          [(name, 1000) for name in ('antipodal', 'short', 'pole', 'poles', 'meridional',
                                     'equatorial', 'vertices', 'near-vertices')])
HEADER = """\
# A sample of C. F. F. Karney's "Test set for geodesics" (2010),
# doi:10.5281/zenodo.32156 (Creative Commons Zero v1.0), from GeographicLib's
# GeodTest-short.dat (SHA-256 601de5a4...14734710), written by
# tools/references/geodtest_sample.py: every 20th line, with its group and its line
# number there. WGS84; degrees and metres; point 2 to 10^-18 degrees, s12
# exact, S12 the area between the geodesic and the equator to 1 mm².
# group	line	lat1	lon1	azi1	lat2	lon2	s12	a12	m12	S12
"""


def decimal(text: str) -> str:
    """A test-set number as Jadpo writes a decimal: `.5` is `0.5`."""
    if text.startswith('-.'):
        return '-0.' + text[2:]
    if text.startswith('.'):
        return '0.' + text[1:]
    return text


def sample(source: Path) -> str:
    data = source.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != SHA256:
        sys.exit(f'{source} is not GeodTest-short.dat: its SHA-256 is {digest}')
    lines = data.decode('ascii').splitlines()
    groups = [name for name, count in GROUPS for _ in range(count)]
    if len(lines) != len(groups):
        sys.exit(f'{source} has {len(lines)} lines, not {len(groups)}')
    out = [HEADER]
    for at in range(0, len(lines), EVERY):
        fields = lines[at].split()
        lat1, lon1, azi1, lat2, lon2, _azi2, s12, a12, m12, area = fields
        row = [groups[at], str(at + 1)] + [decimal(f) for f in
                                            (lat1, lon1, azi1, lat2, lon2, s12, a12, m12, area)]
        out.append('\t'.join(row) + '\n')
    return ''.join(out)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('source', type=Path, help='GeodTest-short.dat, unzipped')
    parser.add_argument('--check', action='store_true',
                        help='fail if the committed sample is not what this writes')
    args = parser.parse_args()
    text = sample(args.source)
    if args.check:
        if OUTPUT.read_text() != text:
            sys.exit(f'{OUTPUT.relative_to(ROOT)} is stale: run tools/references/geodtest_sample.py')
        print(f'{OUTPUT.relative_to(ROOT)} is in step with its source')
        return
    OUTPUT.write_text(text)
    print(f'wrote {OUTPUT.relative_to(ROOT)}')


if __name__ == '__main__':
    main()
