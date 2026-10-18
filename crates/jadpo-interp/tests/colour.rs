//! Area 19, "Colour" (chunk 5.5p), run: the page's example
//! (`project=colour`), a design system's brand colour read from a form and
//! written back as CSS writes it, body text checked against WCAG 2's
//! contrast ratio, a hover colour and a tint worked out in OKLCH, and HSL
//! and OKLCH swatches, giving the values CSS Color 4's sample code gives.
//! The runtime's `colour` tests hold the same operations to many more of
//! CSS's values.

use crate::support;

use jadpo_interp::Value;
use jadpo_std::QuantityKind;
use jadpo_value::{Decimal, units};
use support::{failure, main, text, value};

const SOURCE: &str = "type FormText = Text {}

type YesNo = Bool {}

type SwatchHue = Angle {}

type SwatchShare = Percentage {
    min: 0%
    max: 100%
}

type SwatchChroma = Decimal {
    scale: 4
    min: 0.0000
}

type NotAColour = Failure {
    kind: FailureKind.invalid_value
}

/// The brand colour a settings form names.
function settings_colour(raw: FormText) fails NotAColour -> Colour {
    var typed_colour: Colour = attempt text.parse(raw, as: Colour) {
        malformed: NotAColour
    }
    return typed_colour
}

/// A colour as a style sheet writes it.
function css_colour(swatch: Colour) -> FormText {
    return FormText(text.from(swatch))
}

/// Whether body text in `text_colour` on `page_colour` meets WCAG 2's AA.
function body_text_passes(text_colour: Colour, page_colour: Colour) -> YesNo {
    return YesNo(colour.contrast(text_colour, page_colour) >= 4.5)
}

/// A button's hover colour: its hue, lighter.
function hover_colour(button_colour: Colour) -> Colour {
    return colour.with_lightness(button_colour, 75%)
}

/// A tint of a colour: a fifth of the way to white.
function tint(swatch: Colour) -> Colour {
    var white: Colour = Colour {
        red: 255
        green: 255
        blue: 255
        alpha: 255
    }
    return colour.mix(swatch, white, 20%)
}

/// A designer's HSL swatch.
function hsl_swatch(
    swatch_hue: SwatchHue,
    swatch_saturation: SwatchShare,
    swatch_lightness: SwatchShare
) -> Colour {
    return colour.from_hsl(swatch_hue, swatch_saturation, swatch_lightness)
}

/// A designer's OKLCH swatch.
function oklch_swatch(
    swatch_lightness: SwatchShare,
    swatch_chroma: SwatchChroma,
    swatch_hue: SwatchHue
) -> Colour {
    return colour.from_oklch(swatch_lightness, swatch_chroma, swatch_hue)
}
";

fn percent(n: i128) -> Value {
    Value::Percentage(Decimal::new(n, 2))
}

/// An angle of whole degrees, held in radians to 9 places as `120deg` is.
fn degrees(n: i64) -> Value {
    let radians = n as f64 * std::f64::consts::PI / 180.0;
    units::value(QuantityKind::Angle, (radians * 1e9).round() as i128)
}

#[test]
fn a_colour_is_read_and_written_as_css_writes_it() {
    let program = main(SOURCE).with_proof_checks();
    let brand = value(&program, "settings_colour", vec![text("#1E6FD9")]);
    assert_eq!(value(&program, "css_colour", vec![brand]), text("#1e6fd9"));
    let translucent = value(&program, "settings_colour", vec![text("#1e6fd980")]);
    assert_eq!(
        value(&program, "css_colour", vec![translucent]),
        text("#1e6fd980")
    );
    for raw in ["#1e6", "#1e6f", "1e6fd9", "#1e6fd9f", "rgb(30 111 217)", ""] {
        assert_eq!(
            failure(&program, "settings_colour", vec![text(raw)]).name,
            "NotAColour",
            "{raw}"
        );
    }
}

#[test]
fn body_text_is_checked_by_wcag_2_s_ratio() {
    let program = main(SOURCE).with_proof_checks();
    let colour = |raw: &str| value(&program, "settings_colour", vec![text(raw)]);
    let readable = |ink: &str, paper: &str| {
        value(
            &program,
            "body_text_passes",
            vec![colour(ink), colour(paper)],
        ) == Value::Bool(true)
    };
    assert!(readable("#000000", "#ffffff"));
    assert!(readable("#767676", "#ffffff"), "4.54:1");
    assert!(!readable("#777777", "#ffffff"), "4.48:1, never rounded up");
    assert!(!readable("#ff0000", "#ffffff"), "3.998:1");
    assert!(readable("#ffffff", "#0000ff"), "8.59:1, either way round");
}

#[test]
fn hover_tint_and_swatches_are_css_s() {
    let program = main(SOURCE).with_proof_checks();
    let shown = |colour: Value| value(&program, "css_colour", vec![colour]);
    let brand = value(&program, "settings_colour", vec![text("#1e6fd9")]);
    assert_eq!(
        shown(value(&program, "hover_colour", vec![brand.clone()])),
        text("#5fadff")
    );
    assert_eq!(shown(value(&program, "tint", vec![brand])), text("#518de3"));
    let hsl = |h: i64, s: i128, l: i128| {
        shown(value(
            &program,
            "hsl_swatch",
            vec![degrees(h), percent(s), percent(l)],
        ))
    };
    assert_eq!(hsl(120, 100, 25), text("#008000"));
    assert_eq!(hsl(210, 75, 45), text("#1d73c9"));
    assert_eq!(hsl(30, 50, 60), text("#cc9966"));
    let oklch = |l: i128, c: i128, h: i64| {
        shown(value(
            &program,
            "oklch_swatch",
            vec![
                Value::Percentage(Decimal::new(l, 3)),
                Value::Decimal(Decimal::new(c, 4)),
                degrees(h),
            ],
        ))
    };
    assert_eq!(oklch(700, 1000, 200), text("#40b1b7"));
    // Out of sRGB's gamut, clipped channel by channel.
    assert_eq!(oklch(900, 3000, 140), text("#4bff00"));
}
