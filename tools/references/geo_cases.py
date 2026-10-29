#!/usr/bin/env python3
"""Writes the cases area 20's predicates and shapes are held to (chunk
5.5u3), with PostGIS's answer to each:
crates/jadpo-value/examples/geodesics/shapes.tsv.

  python3 tools/references/geo_cases.py --url postgresql://postgres@127.0.0.1:5432/postgres
  python3 tools/references/geo_cases.py --url ... --check   # fail if the file is stale

It needs a PostgreSQL server with PostGIS that it may `CREATE EXTENSION
postgis` in, reached with `psql` (on PATH, or Postgres.app's). The gate
doesn't run it: the cases are committed with PostGIS's answers, and the
file's header records the PostGIS and PostgreSQL versions that gave them,
so CI needs no PostGIS for them (crates/jadpo-value/examples/geodesics/
shapes.rs checks Jadpo against the file, natively and under WebAssembly).

Every case is away from a boundary, as the chunk's "done when" asks: no
point within a few metres of an edge it is tested against, except where
both sides are exact (a polygon's own corner, a point on a meridian or
the equator). Each geometry is written once here, latitude first, and
sent to PostGIS as WKT and to Jadpo as GeoJSON, both longitude first, from
the same decimal text.

What each operation is held to:

- `intersects`, `contains`, `within`: `ST_Intersects`, `ST_Covers` and
  `ST_CoveredBy` on `geography`, the SQL forms area 20 names. A
  multi-part case is one where every part of `b` lies in one part of `a`,
  where `ST_Covers`'s part-by-part answer is the page's "no point of `b`
  outside `a`".
- `within_distance`: `ST_DWithin(a, b, d)` on the ellipsoid, two points.
- `circle`: `ST_DWithin(centre, b, radius)`, a `Circle` holding a point.
- `box` and `bounding_box`: PostGIS has no box of parallels and meridians
  for a `geography`, so the shape is cut by `ST_Segmentize` into points
  at most 100 m apart along its great-circle edges, with `ST_Covers` saying
  whether it holds a pole, and the box is the one around those points,
  its longitudes the complement of the widest gap between them (the gap
  across the antimeridian winning a tie). A box's side worked out from an
  edge is then within 10⁻⁸ of a degree of the edge's highest point, and
  the check allows 10⁻⁶.
- `centroid`: `ST_Centroid(x, false)`, the check allowing 10⁻⁹ of a degree.

A few cases are where the page's meaning and PostGIS's answer part, each
listed in `DIFFERS` with the page's answer and why; the file records both,
and Jadpo is held to the page's. They are `ST_Covers` comparing a polygon
with a polygon ring by ring, so a hole of `a` that `b` covers isn't seen,
and a ring of `b` lying along one of `a`'s counts as crossing it; and
`ST_Covers` with a multi-part `a`, which wants all of `b` in one part.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'crates/jadpo-value/examples/geodesics/shapes.tsv'
SEGMENT = 100  # metres between ST_Segmentize's points

# Geometries, latitude then longitude, as decimal text. A ring is listed
# without its closing point, which is added.
SHAPES = {
    # Points.
    'big_ben': ('point', '51.5007 -0.1246'),
    'paris': ('point', '48.8566 2.3522'),
    'soho': ('point', '51.5136 -0.1365'),
    'greenwich': ('point', '51.4769 0'),
    'square_middle': ('point', '5 5'),
    'square_corner': ('point', '10 10'),
    'square_outside': ('point', '-1 5'),
    'in_hole': ('point', '3 3'),
    'beside_hole': ('point', '1 7'),
    'on_meridian_edge': ('point', '3.25 0'),
    'on_equator_edge': ('point', '0 7.125'),
    'band_north': ('point', '60.1 0'),
    'band_south': ('point', '50.1 0'),
    'band_inside': ('point', '55 3'),
    'across_east': ('point', '5 -175'),
    'across_west': ('point', '-5 175'),
    'across_far': ('point', '0 0'),
    'antimeridian': ('point', '0 180'),
    'north_pole': ('point', '90 0'),
    'south_pole': ('point', '-90 0'),
    'arctic': ('point', '85 -45'),
    'subarctic': ('point', '75 -45'),
    'antarctic': ('point', '-80 77'),
    'big_inside': ('point', '35 0'),
    'big_outside': ('point', '45 0'),
    'big_bulge': ('point', '41.5 0'),
    'equator_inside': ('point', '0.5 25'),
    'l_inside': ('point', '1 5'),
    'l_notch': ('point', '5 5'),
    'near_ben': ('point', '51.5052 -0.1246'),
    'far_from_ben': ('point', '51.5142 -0.1246'),
    # Multi-points.
    'two_in_square': ('multipoint', ['5 5', '8 1']),
    'one_in_hole': ('multipoint', ['5 5', '3 3']),
    'london_pins': ('multipoint', ['51.5007 -0.1246', '51.5136 -0.1365', '51.5033 -0.1195']),
    'spread': ('multipoint', ['0 0', '0 10', '10 0']),
    'one_in_each': ('multipoint', ['20.5 20.5', '0 180']),
    # Lines.
    'meridian': ('line', ['0 5', '10 5']),
    'meridian_part': ('line', ['2 5', '4 5']),
    'meridian_past': ('line', ['2 5', '11 5']),
    'equator_line': ('line', ['0 0', '0 10']),
    'equator_part': ('line', ['0 2.5', '0 7.5']),
    'bend': ('line', ['0 0', '0 5', '5 5']),
    'bend_part': ('line', ['0 2', '0 5', '3 5']),
    'inside_square': ('line', ['1 6', '9 8', '6 9']),
    'through_hole': ('line', ['1 1', '5 5']),
    'leaving_square': ('line', ['5 5', '20 20']),
    'far_line': ('line', ['11 11', '20 20']),
    'diagonal': ('line', ['0 0', '10 10']),
    'other_diagonal': ('line', ['0 10', '10 0']),
    'across_line': ('line', ['0 175', '0 -175']),
    'polar_line': ('line', ['85 0', '85 180']),
    'band_top': ('line', ['59.9 -5', '59.9 5']),
    'van_round': ('line', ['51.5007 -0.1246', '51.5136 -0.1365', '51.5033 -0.1195', '51.4769 0']),
    # Multi-lines.
    'two_lines': ('multiline', [['1 6', '9 8'], ['6 1', '9 2']]),
    'round_and_spur': ('multiline', [['51.5007 -0.1246', '51.5136 -0.1365'], ['51.5033 -0.1195', '51.4769 0']]),
    # Polygons.
    'london_patch': ('polygon', [['51.49 -0.2', '51.49 -0.05', '51.54 -0.05', '51.54 -0.2']]),
    'westminster': ('polygon', [['51.495 -0.14', '51.495 -0.115', '51.505 -0.115', '51.505 -0.14']]),
    'square': ('polygon', [['0 0', '0 10', '10 10', '10 0']]),
    'square_hole': ('polygon', [['0 0', '0 10', '10 10', '10 0'], ['2 2', '4 2', '4 4', '2 4']]),
    'small_in_square': ('polygon', [['5 5', '5 6', '6 6']]),
    'over_hole': ('polygon', [['1 1', '1 6', '6 6', '6 1']]),
    'around_hole': ('polygon', [['1 1', '1 6', '6 6', '6 1'], ['2 2', '4 2', '4 4', '2 4']]),
    'in_hole_patch': ('polygon', [['2.5 2.5', '2.5 3.5', '3.5 3.5']]),
    'overlapping': ('polygon', [['8 8', '8 15', '15 15', '15 8']]),
    'disjoint': ('polygon', [['20 20', '20 30', '30 30']]),
    'covering': ('polygon', [['-5 -5', '-5 15', '15 15', '15 -5']]),
    'concave_l': ('polygon', [['0 0', '0 10', '2 10', '2 2', '10 2', '10 0']]),
    'band': ('polygon', [['50 -10', '50 10', '60 10', '60 -10']]),
    'across': ('polygon', [['-10 170', '-10 -170', '10 -170', '10 170']]),
    'across_small': ('polygon', [['-2 178', '-2 -178', '2 -178', '2 178']]),
    'north_cap': ('polygon', [['80 0', '80 90', '80 180', '80 -90']]),
    'south_cap': ('polygon', [['-70 0', '-70 -120', '-70 120']]),
    'big': ('polygon', [['-30 -60', '-30 60', '40 60', '40 -60']]),
    'equator_cross': ('polygon', [['-5 20', '-5 30', '5 30', '5 20']]),
    # Multi-polygons.
    'two_patches': ('multipolygon', [[['20 20', '20 21', '21 21', '21 20']], [['-10 170', '-10 -170', '10 -170', '10 170']]]),
    'patch_and_hole': ('multipolygon', [[['0 0', '0 10', '10 10', '10 0'], ['2 2', '4 2', '4 4', '2 4']], [['30 30', '30 31', '31 31']]]),
}

# The cases where the page's meaning ("no point of `b` lies outside `a`",
# area 20) and PostGIS's `ST_Covers` part, with the page's answer and why.
DIFFERS = {
    ('contains', 'square_hole', 'over_hole'): (
        'f', "b covers a's hole, which ST_Covers on geography doesn't look at"),
    ('contains', 'square_hole', 'around_hole'): (
        't', "b's hole is a's hole; ST_Covers counts rings along each other as crossing"),
    ('contains', 'square', 'square_hole'): (
        't', "b is a with a hole; ST_Covers counts rings along each other as crossing"),
    ('contains', 'two_patches', 'one_in_each'): (
        't', "each point is in one of a's parts; ST_Covers wants both in one part"),
}

# Each case: its operation and what it takes.
CASES = [
    # contains: ST_Covers(a, b).
    ('contains', 'square', 'square_middle'),
    ('contains', 'square', 'square_corner'),
    ('contains', 'square', 'square_outside'),
    ('contains', 'square', 'on_meridian_edge'),
    ('contains', 'square', 'on_equator_edge'),
    ('contains', 'square_hole', 'in_hole'),
    ('contains', 'square_hole', 'beside_hole'),
    ('contains', 'band', 'band_north'),
    ('contains', 'band', 'band_south'),
    ('contains', 'band', 'band_inside'),
    ('contains', 'band', 'band_top'),
    ('contains', 'across', 'across_east'),
    ('contains', 'across', 'across_west'),
    ('contains', 'across', 'antimeridian'),
    ('contains', 'across', 'across_far'),
    ('contains', 'across', 'across_line'),
    ('contains', 'across', 'across_small'),
    ('contains', 'north_cap', 'north_pole'),
    ('contains', 'north_cap', 'arctic'),
    ('contains', 'north_cap', 'subarctic'),
    ('contains', 'north_cap', 'polar_line'),
    ('contains', 'north_cap', 'south_pole'),
    ('contains', 'south_cap', 'south_pole'),
    ('contains', 'south_cap', 'antarctic'),
    ('contains', 'south_cap', 'north_pole'),
    ('contains', 'big', 'big_inside'),
    ('contains', 'big', 'big_outside'),
    ('contains', 'big', 'big_bulge'),
    ('contains', 'big', 'square'),
    ('contains', 'equator_cross', 'equator_inside'),
    ('contains', 'concave_l', 'l_inside'),
    ('contains', 'concave_l', 'l_notch'),
    ('contains', 'london_patch', 'big_ben'),
    ('contains', 'london_patch', 'paris'),
    ('contains', 'london_patch', 'westminster'),
    ('contains', 'london_patch', 'van_round'),
    ('contains', 'london_patch', 'london_pins'),
    ('contains', 'westminster', 'london_patch'),
    ('contains', 'square', 'two_in_square'),
    ('contains', 'square_hole', 'one_in_hole'),
    ('contains', 'square', 'inside_square'),
    ('contains', 'square_hole', 'through_hole'),
    ('contains', 'square', 'through_hole'),
    ('contains', 'square', 'leaving_square'),
    ('contains', 'square', 'two_lines'),
    ('contains', 'square_hole', 'two_lines'),
    ('contains', 'square', 'small_in_square'),
    ('contains', 'square_hole', 'small_in_square'),
    ('contains', 'square_hole', 'over_hole'),
    ('contains', 'square_hole', 'around_hole'),
    ('contains', 'square_hole', 'in_hole_patch'),
    ('contains', 'square', 'square_hole'),
    ('contains', 'square_hole', 'square'),
    ('contains', 'square', 'overlapping'),
    ('contains', 'covering', 'square'),
    ('contains', 'square', 'covering'),
    ('contains', 'meridian', 'meridian_part'),
    ('contains', 'meridian', 'meridian_past'),
    ('contains', 'meridian', 'square_middle'),
    ('contains', 'equator_line', 'equator_part'),
    ('contains', 'bend', 'bend_part'),
    ('contains', 'square_middle', 'square_middle'),
    ('contains', 'square_middle', 'square_corner'),
    ('contains', 'two_patches', 'across_east'),
    ('contains', 'two_patches', 'across_small'),
    ('contains', 'two_patches', 'square_middle'),
    ('contains', 'patch_and_hole', 'in_hole'),
    ('contains', 'patch_and_hole', 'beside_hole'),
    ('contains', 'patch_and_hole', 'small_in_square'),
    ('contains', 'two_patches', 'one_in_each'),
    # within: ST_CoveredBy(a, b).
    ('within', 'big_ben', 'london_patch'),
    ('within', 'paris', 'london_patch'),
    ('within', 'westminster', 'london_patch'),
    ('within', 'in_hole', 'square_hole'),
    ('within', 'meridian_part', 'meridian'),
    # intersects: ST_Intersects(a, b).
    ('intersects', 'square', 'square_middle'),
    ('intersects', 'square', 'square_outside'),
    ('intersects', 'square', 'leaving_square'),
    ('intersects', 'square', 'far_line'),
    ('intersects', 'square', 'overlapping'),
    ('intersects', 'square', 'disjoint'),
    ('intersects', 'square', 'covering'),
    ('intersects', 'square_hole', 'in_hole_patch'),
    ('intersects', 'square_hole', 'in_hole'),
    ('intersects', 'square_hole', 'over_hole'),
    ('intersects', 'diagonal', 'other_diagonal'),
    ('intersects', 'diagonal', 'far_line'),
    ('intersects', 'across', 'across_line'),
    ('intersects', 'across', 'across_small'),
    ('intersects', 'across', 'square'),
    ('intersects', 'north_cap', 'polar_line'),
    ('intersects', 'north_cap', 'north_pole'),
    ('intersects', 'south_cap', 'north_cap'),
    ('intersects', 'band', 'band_top'),
    ('intersects', 'band', 'band_south'),
    ('intersects', 'london_patch', 'van_round'),
    ('intersects', 'london_patch', 'paris'),
    ('intersects', 'westminster', 'round_and_spur'),
    ('intersects', 'two_patches', 'across_line'),
    ('intersects', 'two_patches', 'diagonal'),
    ('intersects', 'spread', 'square'),
    ('intersects', 'spread', 'disjoint'),
    ('intersects', 'big', 'equator_cross'),
    ('intersects', 'concave_l', 'small_in_square'),
    # within_distance: ST_DWithin(a, b, metres), on the ellipsoid.
    ('within_distance', 'big_ben', 'paris', '340000'),
    ('within_distance', 'big_ben', 'paris', '345000'),
    ('within_distance', 'big_ben', 'soho', '1500'),
    ('within_distance', 'big_ben', 'soho', '1800'),
    ('within_distance', 'north_pole', 'arctic', '556000'),
    ('within_distance', 'north_pole', 'arctic', '560000'),
    ('within_distance', 'across_east', 'across_west', '1500000'),
    ('within_distance', 'across_east', 'across_west', '1600000'),
    # circle: ST_DWithin(centre, b, radius).
    ('circle', 'big_ben 1000', 'near_ben'),
    ('circle', 'big_ben 1000', 'far_from_ben'),
    ('circle', 'antimeridian 700000', 'across_east'),
    ('circle', 'antimeridian 900000', 'across_east'),
    # box: the box south west north east holds b, from b cut by
    # ST_Segmentize.
    ('box', '51.48 -0.21 51.55 -0.04', 'london_patch'),
    ('box', '51.48 -0.21 51.55 -0.04', 'big_ben'),
    ('box', '51.48 -0.21 51.55 -0.04', 'paris'),
    ('box', '51.48 -0.21 51.55 -0.04', 'van_round'),
    ('box', '49 -11 60.3 11', 'band'),
    ('box', '49 -11 60.5 11', 'band'),
    ('box', '-11 169 11 -169', 'across'),
    ('box', '-11 -169 11 169', 'across'),
    ('box', '-11 169 11 -169', 'across_line'),
    ('box', '79 -180 90 180', 'north_cap'),
    ('box', '79 -100 90 100', 'north_cap'),
    ('box', '84 -1 90 1', 'polar_line'),
    ('box', '84 -1 90 180', 'polar_line'),
    # centroid: ST_Centroid(x, false).
    *[('centroid', name) for name in (
        'big_ben', 'london_pins', 'spread', 'two_in_square', 'van_round', 'bend', 'across_line',
        'round_and_spur', 'two_lines', 'london_patch', 'square', 'square_hole',
        'concave_l', 'band', 'across', 'north_cap', 'south_cap', 'big', 'two_patches',
        'patch_and_hole')],
    # bounding_box: the box round b cut by ST_Segmentize.
    *[('bounding_box', name) for name in (
        'big_ben', 'london_pins', 'spread', 'van_round', 'bend', 'across_line',
        'round_and_spur', 'london_patch', 'square_hole', 'concave_l', 'band', 'across',
        'across_small', 'north_cap', 'south_cap', 'big', 'equator_cross', 'two_patches',
        'patch_and_hole', 'north_pole')],
]


def pair(text):
    latitude, longitude = text.split()
    return latitude, longitude


def lng_lat(text):
    latitude, longitude = pair(text)
    return f'{longitude} {latitude}'


def ring_text(ring):
    return ', '.join(lng_lat(p) for p in ring + [ring[0]])


def wkt(name):
    kind, data = SHAPES[name]
    if kind == 'point':
        return f'POINT({lng_lat(data)})'
    if kind == 'multipoint':
        return 'MULTIPOINT(' + ', '.join(f'({lng_lat(p)})' for p in data) + ')'
    if kind == 'line':
        return 'LINESTRING(' + ', '.join(lng_lat(p) for p in data) + ')'
    if kind == 'multiline':
        return 'MULTILINESTRING(' + ', '.join('(' + ', '.join(lng_lat(p) for p in line) + ')'
                                              for line in data) + ')'
    if kind == 'polygon':
        return 'POLYGON(' + ', '.join(f'({ring_text(r)})' for r in data) + ')'
    if kind == 'multipolygon':
        return 'MULTIPOLYGON(' + ', '.join('(' + ', '.join(f'({ring_text(r)})' for r in polygon) + ')'
                                           for polygon in data) + ')'
    raise ValueError(kind)


def number(text):
    """A JSON number written as the decimal text is."""
    return json.loads(text)


def position(text):
    latitude, longitude = pair(text)
    return [number(longitude), number(latitude)]


def geojson(name):
    kind, data = SHAPES[name]
    ring = lambda r: [position(p) for p in r + [r[0]]]
    if kind == 'point':
        shape = {'type': 'Point', 'coordinates': position(data)}
    elif kind == 'multipoint':
        shape = {'type': 'MultiPoint', 'coordinates': [position(p) for p in data]}
    elif kind == 'line':
        shape = {'type': 'LineString', 'coordinates': [position(p) for p in data]}
    elif kind == 'multiline':
        shape = {'type': 'MultiLineString', 'coordinates': [[position(p) for p in line] for line in data]}
    elif kind == 'polygon':
        shape = {'type': 'Polygon', 'coordinates': [ring(r) for r in data]}
    else:
        shape = {'type': 'MultiPolygon', 'coordinates': [[ring(r) for r in polygon] for polygon in data]}
    return json.dumps(shape, separators=(',', ':'))


def geog(name):
    return f"'SRID=4326;{wkt(name)}'::geography"


def psql_binary():
    found = shutil.which('psql')
    if found:
        return found
    for version in ('16', 'latest'):
        candidate = Path(f'/Applications/Postgres.app/Contents/Versions/{version}/bin/psql')
        if candidate.exists():
            return str(candidate)
    sys.exit('psql not found: install PostgreSQL or put psql on PATH')


def run_sql(url, sql):
    done = subprocess.run([psql_binary(), url, '-X', '-q', '-At', '-F', '\t', '-v', 'ON_ERROR_STOP=1'],
                          input=sql, capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f'psql failed:\n{done.stderr}')
    return done.stdout.splitlines()


def gap_box(longitudes, everywhere):
    """The box's west and east round these longitudes: the complement of
    the widest gap between them, the gap across the antimeridian winning a
    tie; every longitude when `everywhere`."""
    if everywhere:
        return -180.0, 180.0
    values = sorted({180.0 if x == -180.0 else x for x in longitudes})
    best, west, east = values[0] + 360 - values[-1], values[0], values[-1]
    for before, after in zip(values, values[1:]):
        if after - before > best:
            best, west, east = after - before, after, before
    if best == 0:
        return -180.0, 180.0
    return west, east


def held_in_box(south, west, north, east, points):
    width = east - west if west <= east else east - west + 360
    for longitude, latitude in points:
        if latitude < south or latitude > north:
            return False
        if abs(latitude) == 90:
            continue
        if (longitude - west) % 360 > width:
            return False
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--url', required=True, help='a PostgreSQL URL whose server has PostGIS')
    parser.add_argument('--check', action='store_true', help='fail if the committed file differs')
    args = parser.parse_args()
    run_sql(args.url, 'CREATE EXTENSION IF NOT EXISTS postgis;')
    [versions] = run_sql(args.url, "SELECT postgis_lib_version(), split_part(version(), ' ', 2);")
    postgis, postgres = versions.split('\t')
    queries = []
    for n, case in enumerate(CASES):
        operation = case[0]
        if operation == 'contains':
            sql = f'ST_Covers({geog(case[1])}, {geog(case[2])})::text'
        elif operation == 'within':
            sql = f'ST_CoveredBy({geog(case[1])}, {geog(case[2])})::text'
        elif operation == 'intersects':
            sql = f'ST_Intersects({geog(case[1])}, {geog(case[2])})::text'
        elif operation == 'within_distance':
            sql = f'ST_DWithin({geog(case[1])}, {geog(case[2])}, {case[3]})::text'
        elif operation == 'circle':
            centre, radius = case[1].split()
            sql = f'ST_DWithin({geog(centre)}, {geog(case[2])}, {radius})::text'
        elif operation == 'centroid':
            c = f'ST_Centroid({geog(case[1])}, false)::geometry'
            sql = f"ST_Y({c})::text || ' ' || ST_X({c})::text"
        else:
            name = case[2] if operation == 'box' else case[1]
            g = geog(name)
            sql = (f"ST_AsText(ST_Segmentize({g}, {SEGMENT})) || '|' || "
                   f"ST_Covers({g}, 'SRID=4326;POINT(0 90)'::geography)::text || '|' || "
                   f"ST_Covers({g}, 'SRID=4326;POINT(0 -90)'::geography)::text")
        queries.append(f'SELECT {n}, {sql};')
    answers = {}
    for line in run_sql(args.url, '\n'.join(queries)):
        n, answer = line.split('\t', 1)
        answers[int(n)] = answer
    rows = []
    for n, case in enumerate(CASES):
        operation, answer = case[0], answers[n]
        if operation in ('box', 'bounding_box'):
            text, north_pole, south_pole = answer.split('|')
            points = [(float(x), float(y)) for x, y in re.findall(r'(-?[\d.e-]+) (-?[\d.e-]+)', text)]
            if north_pole == 'true':
                points.append((0.0, 90.0))
            if south_pole == 'true':
                points.append((0.0, -90.0))
            everywhere = north_pole == 'true' or south_pole == 'true'
            if operation == 'box':
                south, west, north, east = (float(x) for x in case[1].split())
                if everywhere and not (west == -180 and east == 180):
                    answer = 'f'
                else:
                    answer = 't' if held_in_box(south, west, north, east, points) else 'f'
            else:
                latitudes = [y for _, y in points]
                west, east = gap_box([x for x, y in points if abs(y) != 90] or [points[0][0]], everywhere)
                answer = f'{min(latitudes)!r} {west!r} {max(latitudes)!r} {east!r}'
        elif answer in ('true', 'false'):
            answer = answer[0]
        if operation == 'box':
            a, b, extra = 'box:' + ','.join(case[1].split()), geojson(case[2]), ''
        elif operation == 'circle':
            centre, radius = case[1].split()
            a, b, extra = 'circle:' + geojson(centre) + ',' + radius, geojson(case[2]), ''
        elif operation in ('centroid', 'bounding_box'):
            a, b, extra = geojson(case[1]), '', ''
        else:
            a, b = geojson(case[1]), geojson(case[2])
            extra = case[3] if len(case) > 3 else ''
        label = ' '.join(case[1:])
        page = ''
        if case in DIFFERS:
            wanted, why = DIFFERS[case]
            if wanted == answer:
                sys.exit(f'{label}: listed as differing, but PostGIS gives the page\'s {wanted}')
            page = f'{wanted}: {why}'
        rows.append('\t'.join([operation, label, a, b, extra, answer, page]))
    header = (
        f"# Area 20's predicates and shapes (chunk 5.5u3), each with PostGIS's answer:\n"
        f"# PostGIS {postgis} on PostgreSQL {postgres}, written by tools/references/geo_cases.py,\n"
        f"# which says what each operation is held to. Geometries are GeoJSON;\n"
        f"# `box:` is south,west,north,east, `circle:` a centre and a radius in metres.\n"
        f"# `page` is set where the page's meaning and PostGIS's answer part: the\n"
        f"# page's answer and why, which Jadpo gives.\n"
        f"# operation\tcase\ta\tb\tdistance\tanswer\tpage\n"
    )
    text = header + '\n'.join(rows) + '\n'
    if args.check:
        if OUTPUT.read_text() != text:
            sys.exit(f'{OUTPUT.relative_to(ROOT)} is stale: run tools/references/geo_cases.py --url ...')
        print(f'{OUTPUT.relative_to(ROOT)} matches PostGIS {postgis}')
        return
    OUTPUT.write_text(text)
    print(f'wrote {len(rows)} cases to {OUTPUT.relative_to(ROOT)} (PostGIS {postgis})')


if __name__ == '__main__':
    main()
