#!/usr/bin/env python3
"""Writes the reference values area 19's maths on `Decimal` is tested
against (chunk 5.5e): crates/jadpo-value/examples/decimal_maths/reference.tsv.

  python3 tools/decimal_maths_reference.py          # write the file
  python3 tools/decimal_maths_reference.py --check  # fail if it is stale

Each value is worked out with Python's own `decimal` module at 250
significant digits, independently of Jadpo's Rust code: `sqrt`, `exp` and
`ln` are libmpdec's, which the module documents as correctly rounded; π
is the module documentation's series recipe, checked against 100 published
digits; `sin` and `cos` are their Taylor series after taking out whole
turns; `atan2` is Euler's series for the arctangent. The value is then
rounded to the scale by the mode, and only when every value within
10^-190 of it rounds the same way, so the 250-digit working can't change
a result; a value that ends within 40 places is exact. A float from
Python's `math` module checks each value to 9 places, against a slip in
the series. `power` is exact, worked out in whole numbers.
"""

import argparse
import math
import sys
from decimal import (ROUND_DOWN, ROUND_HALF_EVEN, ROUND_HALF_UP, ROUND_UP,
                     Decimal, getcontext, localcontext)
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'crates/jadpo-value/examples/decimal_maths/reference.tsv'
PRECISION = 250
EPSILON = Decimal('1e-190')
MODES = {'half_even': ROUND_HALF_EVEN, 'half_up': ROUND_HALF_UP, 'up': ROUND_UP,
         'down': ROUND_DOWN}
SCALES = (0, 1, 2, 9, 18)
# π to 100 places, as published (e.g. OEIS A000796).
PI_100 = Decimal('3.1415926535897932384626433832795028841971693993751058209749445923'
                 '078164062862089986280348253421170679')

getcontext().prec = PRECISION


def pi() -> Decimal:
    """The `decimal` module documentation's recipe, at 10 more digits."""
    with localcontext() as ctx:
        ctx.prec = PRECISION + 10
        three = Decimal(3)
        lasts, t, s, n, na, d, da = 0, three, 3, 1, 0, 0, 24
        while s != lasts:
            lasts = s
            n, na = n + na, na + 8
            d, da = d + da, da + 32
            t = (t * n) / d
            s += t
    return +s


PI = pi()
assert abs(PI - PI_100) < Decimal('1e-100'), 'π agrees with its published digits'


def sin_cos(x: Decimal) -> tuple[Decimal, Decimal]:
    """By Taylor series, after taking out whole turns of 2π worked out
    at 60 more digits than the working precision."""
    with localcontext() as ctx:
        ctx.prec = PRECISION + 60
        turn = 2 * pi_wide(ctx.prec)
        r = x - turn * (x / turn).to_integral_value(rounding=ROUND_HALF_EVEN)
        square = r * r
        sin_sum, cos_sum = Decimal(0), Decimal(0)
        term, n = r, 1
        while term != 0 and abs(term) > Decimal(10) ** -(PRECISION + 20):
            sin_sum += term
            term = -term * square / ((n + 1) * (n + 2))
            n += 2
        term, n = Decimal(1), 0
        while term != 0 and abs(term) > Decimal(10) ** -(PRECISION + 20):
            cos_sum += term
            term = -term * square / ((n + 1) * (n + 2))
            n += 2
    return +sin_sum, +cos_sum


def pi_wide(digits: int) -> Decimal:
    with localcontext() as ctx:
        ctx.prec = digits + 10
        three = Decimal(3)
        lasts, t, s, n, na, d, da = 0, three, 3, 1, 0, 0, 24
        while s != lasts:
            lasts = s
            n, na = n + na, na + 8
            d, da = d + da, da + 32
            t = (t * n) / d
            s += t
        return s


def atan(t: Decimal) -> Decimal:
    """Euler's series, `atan t = Σ 2^2n (n!)² / (2n+1)! · t^(2n+1) /
    (1+t²)^(n+1)`, for 0 ≤ t ≤ 1, where its ratio is at most 1/2."""
    with localcontext() as ctx:
        ctx.prec = PRECISION + 20
        ratio = t * t / (1 + t * t)
        term = t / (1 + t * t)
        total, n = Decimal(0), 0
        while term > Decimal(10) ** -(PRECISION + 15):
            total += term
            n += 1
            term = term * ratio * (2 * n) / (2 * n + 1)
    return +total


def atan2(y: Decimal, x: Decimal) -> Decimal:
    if y == 0 and x == 0:
        return Decimal(0)
    if x == 0:
        return PI / 2 if y > 0 else -PI / 2
    a, b = abs(y), abs(x)
    angle = atan(a / b) if a <= b else PI / 2 - atan(b / a)
    if x < 0:
        angle = PI - angle
    return -angle if y < 0 else angle


def rounded(value: Decimal, scale: int, mode: str) -> str:
    """`value` rounded to `scale` places, as Jadpo writes it, or
    `overflow` past 38 digits."""
    quantum = Decimal(1).scaleb(-scale)
    with localcontext() as ctx:
        ctx.prec = PRECISION + 50
        exact = value == value.quantize(Decimal('1e-40'), rounding=ROUND_DOWN)
        if exact:
            result = value.quantize(quantum, rounding=MODES[mode])
        else:
            low = (value - EPSILON).quantize(quantum, rounding=MODES[mode])
            high = (value + EPSILON).quantize(quantum, rounding=MODES[mode])
            if low != high:
                raise SystemExit(f'{value} is too near a boundary at {scale} places')
            result = low
    digits = len(result.as_tuple().digits)
    if digits > 38 and result != 0:
        return 'overflow'
    text = f'{result:f}'
    # Zero below the point keeps its places but no sign: `-0.00` is `0.00`.
    return text[1:] if text.startswith('-') and result == 0 else text


def sanity(name: str, value: Decimal, approximate: float | None) -> None:
    if approximate is None or abs(value) > Decimal('1e15'):
        return
    if abs(float(value) - approximate) > 1e-9 * max(1.0, abs(approximate)):
        raise SystemExit(f'{name}: {value} is not near {approximate}')


def maths_cases() -> list[tuple[str, list[str], Decimal | str, float | None]]:
    """(operation, arguments, true value or failure, a float to check)."""
    cases = []
    for raw in ['2', '3', '0.5', '10', '0.0625', '1.21', '0', '4.00', '12345.6789', '0.001',
                '0.00000000000000000000000000000000000001',
                '99999999999999999999999999999999999999']:
        x = Decimal(raw)
        cases.append(('sqrt', [raw], x.sqrt(), math.sqrt(float(x))))
    for raw in ['1', '-1', '0.5', '2.302585092994045684', '10', '-10', '20.5', '43.7', '87',
                '0', '-49', '-60', '88', '100', '0.000001', '-0.000001', '87.4']:
        x = Decimal(raw)
        approximate = math.exp(float(x)) if float(x) < 700 else None
        cases.append(('exp', [raw], x.exp(), approximate))
    for raw in ['2', '10', '0.5', '1', '1.000001', '2.718281828459045235', '123.456',
                '0.00000000000000000000000000000000000001',
                '99999999999999999999999999999999999999', '0', '-1']:
        x = Decimal(raw)
        if x <= 0:
            cases.append(('ln', [raw], 'out_of_range', None))
        else:
            cases.append(('ln', [raw], x.ln(), math.log(float(x))))
    for raw in ['1', '-1', '0.5', '3.14159265358979323846', '1.5707963267948966',
                '100', '-100', '10000000000000000000000000000000000000',
                '12345678901234567890.123456789', '0', '6.283185307179586',
                '0.000000000000000000000000000000000001']:
        x = Decimal(raw)
        s, c = sin_cos(x)
        small = abs(float(x)) < 1e6
        cases.append(('sin', [raw], s, math.sin(float(x)) if small else None))
        cases.append(('cos', [raw], c, math.cos(float(x)) if small else None))
    for y, x in [('1', '1'), ('1', '-1'), ('-1', '-1'), ('-1', '1'), ('0', '0'), ('0', '1'),
                 ('0', '-1'), ('1', '0'), ('-1', '0'), ('3', '4'), ('-0.5', '2.25'),
                 ('2', '1'), ('0.00000000000000000000000000000000000001', '1'),
                 ('1', '0.00000000000000000000000000000000000001'),
                 ('10000000000000000000000000000000000000', '-1')]:
        value = atan2(Decimal(y), Decimal(x))
        cases.append(('atan2', [y, x], value, math.atan2(float(y), float(x))))
    cases.append(('pi', [], PI, math.pi))
    return cases


def power(raw: str, exponent: int) -> str:
    """Exact: the digits to the power, the places times it, trailing zeros
    past the value's own places dropped."""
    value = Decimal(raw)
    sign, digits, exponent_of = value.as_tuple()
    places = max(-exponent_of, 0)
    mantissa = int(''.join(map(str, digits))) * 10 ** max(exponent_of, 0)
    if exponent == 0:
        result, scale = 10 ** places, places
    else:
        # Trailing zeros among the places first, so (-1.0)^1000001 stays
        # small; then back to at least the value's own places.
        base, own = mantissa, places
        while own > 0 and base % 10 == 0 and base != 0:
            base //= 10
            own -= 1
        result, scale = base ** exponent, own * exponent
        if scale < places:
            result, scale = result * 10 ** (places - scale), places
        while scale > places and result % 10 == 0:
            result //= 10
            scale -= 1
    if len(str(result)) > 38 or scale > 38:
        return 'overflow'
    negative = sign == 1 and exponent % 2 == 1 and result != 0
    text = str(result).rjust(scale + 1, '0')
    text = f'{text[:-scale]}.{text[-scale:]}' if scale else text
    return ('-' if negative else '') + text


POWERS = [('1.5', 3), ('1.50', 2), ('-2', 3), ('-2', 4), ('1.50', 0), ('0.1', 38),
          ('0.1', 39), ('10', 37), ('10', 38), ('2', 126), ('2', 127), ('0', 5),
          ('-1.0', 1000001), ('1.0001', 100), ('99', 20), ('1.1', 25), ('0.00', 3)]


def lines() -> list[str]:
    out = [
        '# Reference values for area 19\'s maths on `Decimal` (chunk 5.5e), written by',
        '# tools/decimal_maths_reference.py: Python 3\'s `decimal` module at 250 digits',
        '# (libmpdec\'s correctly rounded sqrt, exp and ln; π by the module\'s documented',
        '# series, checked against 100 published digits; sin and cos by Taylor series;',
        '# atan2 by Euler\'s series), rounded to the scale by the mode only where every',
        '# value within 10^-190 rounds alike. `power` is exact, in whole numbers.',
        '# operation\targuments\tscale\trounding\texpected',
    ]
    for name, args, value, approximate in maths_cases():
        if isinstance(value, Decimal):
            sanity(f'{name}({", ".join(args)})', value, approximate)
        scales = range(19) if name == 'pi' else SCALES
        for scale in scales:
            for mode in MODES:
                expected = value if isinstance(value, str) else rounded(value, scale, mode)
                out.append('\t'.join([name, ' '.join(args) or '-', str(scale), mode, expected]))
    for raw, exponent in POWERS:
        out.append('\t'.join(['power', f'{raw} {exponent}', '-', '-', power(raw, exponent)]))
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--check', action='store_true', help='fail if the file is stale')
    args = parser.parse_args()
    text = '\n'.join(lines()) + '\n'
    if args.check:
        if OUTPUT.read_text() != text:
            print(f'{OUTPUT.relative_to(ROOT)} is stale: run {Path(__file__).name}', file=sys.stderr)
            return 1
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(text)
    print(f'wrote {len(text.splitlines())} lines to {OUTPUT.relative_to(ROOT)}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
