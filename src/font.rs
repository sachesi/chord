//! The font of the text, as a stylesheet: the system's monospace font or the one chosen
//! in the preferences, at the zoom of the moment.

use std::fmt::Write;

use glib::translate::IntoGlib;
use gtk::pango;

/// What a font without a size of its own is shown at, so that zooming still has
/// something to scale.
const DEFAULT_POINTS: f64 = 11.0;

pub const MIN_ZOOM: f64 = 0.3;
pub const MAX_ZOOM: f64 = 4.0;

/// The rule that gives views of the class `chord-view` `font` at `zoom`.
pub fn css(font: &pango::FontDescription, zoom: f64) -> String {
    let families: Vec<String> = font
        .family()
        .iter()
        .flat_map(|family| family.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| {
            let name = name.replace(|c: char| c == '"' || c == '\\' || c.is_control(), "");
            format!("\"{name}\"")
        })
        .collect();
    let mut css = format!(
        "textview.chord-view {{ font-family: {};",
        families.join(", ")
    );
    let (size, unit) = match font.size() {
        0 => (DEFAULT_POINTS, "pt"),
        size if font.is_size_absolute() => (f64::from(size) / f64::from(pango::SCALE), "px"),
        size => (f64::from(size) / f64::from(pango::SCALE), "pt"),
    };
    let _ = write!(css, " font-size: {:.2}{unit};", size * zoom);
    if font.set_fields().contains(pango::FontMask::WEIGHT) {
        let _ = write!(css, " font-weight: {};", font.weight().into_glib());
    }
    match font.style() {
        pango::Style::Italic => css.push_str(" font-style: italic;"),
        pango::Style::Oblique => css.push_str(" font-style: oblique;"),
        _ => {}
    }
    css.push_str(" }");
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_name_becomes_family_size_and_weight() {
        let font = pango::FontDescription::from_string("Fira Code Bold 12");
        assert_eq!(
            css(&font, 1.0),
            "textview.chord-view { font-family: \"Fira Code\"; font-size: 12.00pt; font-weight: 700; }"
        );
    }

    #[test]
    fn the_zoom_scales_the_size() {
        let font = pango::FontDescription::from_string("Monospace 10");
        assert!(css(&font, 1.5).contains("font-size: 15.00pt;"));
    }

    #[test]
    fn a_font_without_a_size_gets_one_to_zoom() {
        let font = pango::FontDescription::from_string("Monospace");
        assert!(css(&font, 2.0).contains("font-size: 22.00pt;"));
    }

    #[test]
    fn quotes_in_a_family_cannot_end_its_string() {
        let mut font = pango::FontDescription::new();
        font.set_family("Evil\" } textview { color: red");
        font.set_size(10 * pango::SCALE);
        assert_eq!(
            css(&font, 1.0),
            "textview.chord-view { font-family: \"Evil } textview { color: red\"; font-size: 10.00pt; }"
        );
    }
}
