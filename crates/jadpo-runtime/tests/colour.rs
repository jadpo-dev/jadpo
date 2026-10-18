//! Area 19, "Colour" (chunk 5.5p), in the runtime: `colour`'s operations as
//! compiled code calls them (`jadpo_value::run`). The page's test
//! (`project=colour`) and the interpreter's `colour` tests assert the same
//! values. The reference values are CSS Color 4's sample code
//! (`conversions.js`: `hslToRgb`, `lin_sRGB`, `gam_sRGB`, the XYZ and OKLab
//! matrices) and CSS Color 5's `color-mix()` run in f64, as culori and the
//! browsers run them, each channel clipped and rounded half up; the
//! contrast ratios are WCAG 2's formula with its own reference pairs (black
//! on white 21:1, the 4.5:1 grey `#767676`, red on white 3.998:1). Cases
//! whose f64 value lies within a millionth of a rounding boundary are left
//! out of the generated tables, since an f64 can't say which side it is on.
//! Its text, JSON and columns are the one harness's (`standard_types`).

use jadpo_std::{Op, QuantityKind};
use jadpo_value::{Decimal, Value, colour, run, standard, units};

fn hex(text: &str) -> Value {
    standard::parse(text, "Colour").unwrap_or_else(|fault| panic!("`{text}`: {fault:?}"))
}

fn written(value: &Value) -> String {
    match standard::from(value) {
        Ok(Value::Text(text)) => text.to_string(),
        other => panic!("{other:?}"),
    }
}

fn call(op: Op, args: Vec<Value>) -> Value {
    run(op, &args, None, &mut |_| None).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

/// An angle of whole degrees, held in radians to 9 places as `120deg` is.
fn degrees(n: i64) -> Value {
    let radians = n as f64 * std::f64::consts::PI / 180.0;
    units::value(QuantityKind::Angle, (radians * 1e9).round() as i128)
}

fn percent(n: i64) -> Value {
    Value::Percentage(Decimal::new(i128::from(n), 2))
}

fn hsl(h: i64, s: i64, l: i64) -> String {
    written(&call(
        Op::ColourFromHsl,
        vec![degrees(h), percent(s), percent(l)],
    ))
}

fn oklch(l: i64, c: i64, h: i64) -> String {
    written(&call(
        Op::ColourFromOklch,
        vec![
            percent(l),
            Value::Decimal(Decimal::new(i128::from(c), 4)),
            degrees(h),
        ],
    ))
}

fn contrast(a: &str, b: &str) -> Value {
    call(Op::ColourContrast, vec![hex(a), hex(b)])
}

fn hundredths(text: &str) -> Value {
    Value::Decimal(Decimal::parse(text).expect("a decimal"))
}

#[test]
fn the_page_s_values() {
    assert_eq!(written(&hex("#1E6FD9")), "#1e6fd9");
    assert_eq!(contrast("#000000", "#ffffff"), hundredths("21.00"));
    assert_eq!(contrast("#ffffff", "#ffffff"), hundredths("1.00"));
    assert_eq!(contrast("#767676", "#ffffff"), hundredths("4.54"));
    let brand = hex("#1e6fd9");
    assert_eq!(
        written(&call(
            Op::ColourWithLightness,
            vec![brand.clone(), percent(75)]
        )),
        "#5fadff"
    );
    assert_eq!(
        written(&call(
            Op::ColourMix,
            vec![brand, hex("#ffffff"), percent(20)]
        )),
        "#518de3"
    );
    assert_eq!(hsl(120, 100, 25), "#008000");
    assert_eq!(hsl(210, 75, 45), "#1d73c9");
    // oklch(62.8% 0.2577 29.23) is CSS Color 4's red.
    let hue = units::value(QuantityKind::Angle, 510_159_740);
    assert_eq!(
        written(&call(
            Op::ColourFromOklch,
            vec![
                Value::Percentage(Decimal::new(628, 3)),
                Value::Decimal(Decimal::new(2577, 4)),
                hue,
            ]
        )),
        "#ff0000"
    );
}

#[test]
fn contrast_matches_wcag_2_s_reference_ratios() {
    // WCAG 2's formula's own reference points: the extremes, the lightest
    // grey with 4.5:1 on white (#767676, 4.54:1) and the one just below
    // (#777777, 4.48:1, which rounded down is 4.47 and fails), pure red
    // on white (3.998:1, which never rounds up to 4), blue on white
    // (8.59:1) and #595959 on white (7.00:1, AAA).
    let cases = [
        ("#000000", "#ffffff", "21.00"),
        ("#ffffff", "#000000", "21.00"),
        ("#767676", "#ffffff", "4.54"),
        ("#777777", "#ffffff", "4.47"),
        ("#ff0000", "#ffffff", "3.99"),
        ("#0000ff", "#ffffff", "8.59"),
        ("#595959", "#ffffff", "7.00"),
        ("#00ff00", "#000000", "15.30"),
        ("#123456", "#123456", "1.00"),
    ];
    for (a, b, ratio) in cases {
        assert_eq!(contrast(a, b), hundredths(ratio), "{a} on {b}");
    }
    // Alpha isn't read.
    assert_eq!(contrast("#00000080", "#ffffff"), hundredths("21.00"));
    for (a, b, ratio) in CONTRASTS {
        assert_eq!(contrast(a, b), hundredths(ratio), "{a} on {b}");
    }
}

#[test]
fn hsl_is_css_color_4_s() {
    for (h, s, l, expected) in HSL {
        assert_eq!(hsl(*h, *s, *l), *expected, "hsl({h} {s}% {l}%)");
    }
    // Out of range: hues wrap, percentages clamp.
    assert_eq!(hsl(480, 100, 25), "#008000");
    assert_eq!(hsl(-240, 100, 25), "#008000");
    assert_eq!(hsl(0, 150, 50), "#ff0000");
    assert_eq!(hsl(0, 100, -10), "#000000");
}

#[test]
fn oklch_is_css_color_4_s_clipped() {
    for (l, c, h, expected) in OKLCH {
        assert_eq!(oklch(*l, *c, *h), *expected, "oklch({l}% 0.{c:04} {h})");
    }
    // A chroma below 0 is 0: a grey; above 1, 1.
    assert_eq!(oklch(50, -100, 30), oklch(50, 0, 0));
    assert_eq!(oklch(100, 0, 0), "#ffffff");
    assert_eq!(oklch(0, 0, 0), "#000000");
}

#[test]
fn lightness_and_mixes_are_css_s() {
    for (colour, l, expected) in LIGHTNESS {
        assert_eq!(
            written(&call(
                Op::ColourWithLightness,
                vec![hex(colour), percent(*l)]
            )),
            *expected,
            "{colour} at {l}%"
        );
    }
    for (a, b, p, expected) in MIXES {
        assert_eq!(
            written(&call(Op::ColourMix, vec![hex(a), hex(b), percent(*p)])),
            *expected,
            "{a} and {p}% {b}"
        );
    }
    // A mix of nothing of `b` is `a`, of all of it `b`; a grey's hue is
    // powerless, so blue keeps its own.
    for colour in ["#1e6fd9", "#ff000080", "#808080"] {
        assert_eq!(
            written(&call(
                Op::ColourMix,
                vec![hex(colour), hex("#ffffff"), percent(0)]
            )),
            colour
        );
        assert_eq!(
            written(&call(
                Op::ColourMix,
                vec![hex("#ffffff"), hex(colour), percent(100)]
            )),
            colour
        );
    }
    assert_eq!(
        written(&call(
            Op::ColourMix,
            vec![hex("#ffffff"), hex("#0000ff"), percent(50)]
        )),
        "#74a3ff"
    );
    let _ = colour::colour_value([0, 0, 0, 255]);
}

const HSL: &[(i64, i64, i64, &str)] = &[
    (46, 48, 67, "#d3c082"),
    (293, 62, 69, "#d67fe1"),
    (232, 36, 0, "#000000"),
    (287, 80, 90, "#f1d1fa"),
    (322, 85, 17, "#500735"),
    (305, 37, 94, "#f5eaf4"),
    (60, 76, 81, "#f3f3aa"),
    (275, 29, 41, "#6e4a87"),
    (310, 94, 71, "#fb70e3"),
    (165, 7, 44, "#687874"),
    (200, 17, 95, "#f0f3f4"),
    (23, 55, 89, "#f2dfd4"),
    (74, 75, 77, "#dcf098"),
    (209, 76, 64, "#5da6e9"),
    (236, 91, 34, "#0812a6"),
    (139, 80, 67, "#68ee92"),
    (310, 62, 88, "#f3cded"),
    (357, 39, 70, "#d09598"),
    (72, 35, 76, "#cfd7ac"),
    (103, 64, 89, "#dbf5d1"),
    (90, 60, 28, "#47721d"),
    (137, 85, 70, "#71f496"),
    (116, 4, 38, "#5e655d"),
    (67, 14, 90, "#e8e9e2"),
    (86, 42, 61, "#a1c572"),
    (117, 76, 92, "#ddfadb"),
    (56, 82, 100, "#ffffff"),
    (55, 23, 10, "#1f1e14"),
    (194, 2, 42, "#696c6d"),
    (161, 15, 26, "#384c46"),
    (244, 27, 42, "#524e88"),
    (155, 68, 45, "#25c180"),
    (30, 2, 48, "#7d7a78"),
    (277, 89, 19, "#3b055c"),
    (216, 23, 80, "#c0cad8"),
    (103, 16, 76, "#beccb8"),
    (18, 70, 100, "#ffffff"),
    (41, 6, 37, "#646059"),
    (316, 95, 34, "#a9047d"),
    (155, 30, 60, "#7ab89e"),
];

const OKLCH: &[(i64, i64, i64, &str)] = &[
    (6, 946, 124, "#000100"),
    (35, 3672, 275, "#4100f4"),
    (30, 192, 46, "#362b26"),
    (30, 1711, 253, "#00237f"),
    (42, 2083, 80, "#843100"),
    (86, 3710, 175, "#00ffc3"),
    (50, 2014, 330, "#9d2298"),
    (52, 2026, 345, "#b21e83"),
    (96, 1513, 4, "#ffc5e6"),
    (22, 2119, 235, "#00096c"),
    (29, 776, 174, "#003728"),
    (45, 2805, 125, "#326700"),
    (53, 1563, 256, "#216ac3"),
    (95, 1104, 23, "#ffd1cc"),
    (60, 1867, 274, "#6170ed"),
    (8, 3658, 1, "#3d0000"),
    (59, 1756, 196, "#009a9f"),
    (55, 662, 147, "#577c5b"),
    (55, 3848, 150, "#00a100"),
    (97, 3876, 301, "#ff95ff"),
    (29, 1130, 288, "#2b1c5f"),
    (42, 3929, 303, "#8100f5"),
    (58, 3223, 10, "#ff004d"),
    (74, 3550, 129, "#5ccc00"),
    (21, 1139, 348, "#380021"),
    (82, 1068, 179, "#6ddbc5"),
    (41, 1104, 206, "#005967"),
    (57, 3623, 45, "#ff0000"),
    (23, 2281, 272, "#1b0085"),
    (34, 3428, 303, "#6400c3"),
    (67, 2033, 146, "#06b43b"),
    (84, 1705, 105, "#dcd025"),
    (26, 2218, 204, "#003554"),
    (73, 1868, 203, "#00c7db"),
    (12, 2125, 166, "#001300"),
    (5, 2676, 261, "#1e005b"),
    (73, 2714, 56, "#ff6100"),
    (27, 3908, 103, "#4c1200"),
    (62, 2653, 130, "#4aa000"),
    (71, 849, 337, "#c48db5"),
];

const LIGHTNESS: &[(&str, i64, &str)] = &[
    ("#effc85da", 34, "#393d00da"),
    ("#01a6e794", 42, "#00549094"),
    ("#8300397e", 11, "#2600017e"),
    ("#f58afb18", 55, "#a63eac18"),
    ("#d7c4ade2", 76, "#c0ae97e2"),
    ("#a92ecf5c", 90, "#ffa4ff5c"),
    ("#eb48e106", 13, "#2f003306"),
    ("#1d926360", 70, "#4bb68460"),
    ("#bf8cd5ea", 85, "#ebb6ffea"),
    ("#51b6b724", 67, "#40a7a824"),
    ("#5390898e", 62, "#56938c8e"),
    ("#459acba8", 81, "#78ccffa8"),
    ("#a754fdf2", 98, "#ffcbfff2"),
    ("#f93e9fec", 72, "#ff51afec"),
    ("#bbd83116", 47, "#51660016"),
    ("#6da233f0", 6, "#000200f0"),
    ("#8f1c25fa", 55, "#b94547fa"),
    ("#a1c687b4", 68, "#81a568b4"),
    ("#2320d99e", 25, "#1c009b9e"),
    ("#95aa9b38", 28, "#1c2e2238"),
    ("#77e44d02", 56, "#158e0002"),
    ("#494e6f7c", 100, "#f5fdff7c"),
    ("#8b688126", 18, "#20061b26"),
    ("#bdb20380", 30, "#3c2d0080"),
    ("#5fac750a", 68, "#5fac750a"),
    ("#f1d65744", 93, "#ffe86b44"),
    ("#f3b029ae", 99, "#ffef75ae"),
    ("#e5ba6bc8", 63, "#aa8230c8"),
    ("#47749d12", 90, "#b3e4ff12"),
    ("#995e3f0c", 10, "#1700000c"),
    ("#5bf8d136", 98, "#80fff036"),
    ("#0dc2d310", 58, "#008e9f10"),
    ("#2f3cc51a", 2, "#11003a1a"),
    ("#41e627d4", 32, "#004900d4"),
    ("#c34079be", 100, "#ffcbffbe"),
    ("#35ca3b58", 68, "#13b82358"),
    ("#1704ed22", 75, "#5796ff22"),
    ("#e96e0f9c", 83, "#ffa0539c"),
    ("#2b882146", 24, "#002e0046"),
    ("#5dd2a3a0", 89, "#82f5c5a0"),
];

const MIXES: &[(&str, &str, i64, &str)] = &[
    ("#ffcc152a", "#b891f6f7", 96, "#c18ef1ef"),
    ("#8293d0", "#ceeffc", 51, "#9ec4e8"),
    ("#0be801", "#e7943d32", 87, "#fc9a004d"),
    ("#397edf", "#f58afb18", 32, "#7073e2b5"),
    ("#d7c4ad", "#7330a92e", 18, "#dbbbaad9"),
    ("#653aeb", "#e106c7", 38, "#972eea"),
    ("#926360", "#debf8cd5", 85, "#d3ae81db"),
    ("#7851b6", "#248d42", 62, "#00889c"),
    ("#898eafbc", "#9acba8", 63, "#7dbcbfe6"),
    ("#a754fd", "#43c0f9", 30, "#847dff"),
    ("#ecb54abb", "#311697", 12, "#e99139c3"),
    ("#a233f0", "#ee8f1c25", 14, "#bd21d2e0"),
    ("#08a1c6", "#b4dd52", 96, "#a9dd5a"),
    ("#d99e7f4c", "#aa9b38", 66, "#be953fc2"),
    ("#77e44d", "#1350494e", 8, "#65e25af1"),
    ("#055a8b", "#81266714", 71, "#6a3f8058"),
    ("#0380b9", "#5fac75", 69, "#0ca58f"),
    ("#98f1d657", "#2d62f3", 54, "#009eddb2"),
    ("#ae4fdc", "#ba6bc8", 37, "#b35ad4"),
    ("#47749d", "#e3e0995e", 6, "#41799df5"),
    ("#556a5b", "#d13637", 7, "#556c55"),
    ("#c2d310", "#0e2f3c", 79, "#00535a"),
    ("#4b2841", "#27d47d72", 45, "#7c4514c0"),
    ("#79be1f", "#35ca3b", 3, "#78be20"),
    ("#961704", "#22b370e9", 80, "#649a25ed"),
    ("#9ca57a2b", "#214607", 24, "#5464295e"),
    ("#d2a3a0", "#1effcc15", 100, "#1effcc15"),
    ("#b891f6f7", "#cd8293", 96, "#ce8296"),
    ("#ceeffc", "#da0be8", 65, "#aa8eff"),
    ("#e7943d", "#8300397e", 97, "#8a0d3c82"),
    ("#f58afb18", "#56d7c4", 80, "#4cd3e9d1"),
    ("#7330a9", "#cf5c65", 7, "#7c32a6"),
    ("#48e106", "#f41d92", 19, "#b9bf00"),
    ("#19debf", "#d5eadb78", 58, "#8de3bdb1"),
    ("#b7248d42", "#90898e", 35, "#a0709084"),
    ("#459acb", "#c166a7", 14, "#5894d3"),
    ("#f243c0f9", "#9fecb5", 61, "#ebb51afd"),
    ("#d83116", "#846da2", 20, "#ce3c57"),
    ("#69ee8f", "#25faab", 99, "#26faab"),
    ("#c687b4", "#522320", 68, "#794049"),
];

const CONTRASTS: &[(&str, &str, &str)] = &[
    ("#9e7f4c", "#95aa9b", "1.51"),
    ("#381176", "#77e44d", "8.60"),
    ("#021350", "#494e6f", "2.14"),
    ("#7c055a", "#8b6881", "2.16"),
    ("#266714", "#bdb203", "3.14"),
    ("#80b9fe", "#5fac75", "1.34"),
    ("#0a7b98", "#f1d657", "3.37"),
    ("#442d62", "#f3b029", "6.14"),
    ("#ae4fdc", "#e5ba6b", "2.33"),
    ("#c86186", "#47749d", "1.29"),
    ("#12e3e0", "#995e3f", "3.25"),
    ("#0c556a", "#5bf8d1", "6.25"),
    ("#3637a4", "#0dc2d3", "4.32"),
    ("#10090e", "#2f3cc5", "2.41"),
    ("#1a4b28", "#41e627", "6.04"),
    ("#d47d72", "#c34079", "1.61"),
    ("#be1f6c", "#35ca3b", "2.68"),
    ("#58b196", "#1704ed", "3.57"),
    ("#22b370", "#e96e0f", "1.15"),
    ("#9ca57a", "#2b8821", "1.73"),
    ("#460734", "#5dd2a3", "8.36"),
    ("#a0591e", "#ffcc15", "3.52"),
    ("#2a1bb8", "#91f6f7", "8.59"),
    ("#64cd82", "#93d0c9", "1.14"),
    ("#ceeffc", "#85da0b", "1.44"),
    ("#e801a6", "#e7943d", "1.74"),
    ("#328300", "#397edf", "1.19"),
    ("#2cf58a", "#fb1871", "2.65"),
    ("#56d7c4", "#ade273", "1.16"),
    ("#30a92e", "#cf5c65", "1.27"),
    ("#3aeb48", "#e106c7", "2.60"),
    ("#f41d92", "#636019", "1.68"),
    ("#debf8c", "#d5eadb", "1.39"),
    ("#7851b6", "#b7248d", "1.00"),
    ("#425390", "#898eaf", "2.28"),
    ("#bc459a", "#cba8c1", "2.22"),
    ("#66a754", "#fdf243", "2.48"),
    ("#c0f93e", "#9fecb5", "1.11"),
    ("#4abbd8", "#311697", "5.54"),
    ("#846da2", "#33f069", "2.94"),
];
