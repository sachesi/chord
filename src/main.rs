mod application;
mod config;
mod document;
mod font;
mod preferences;
mod view;
mod window;

use gio::prelude::*;

fn main() -> glib::ExitCode {
    // SAFETY: the first thing the program does; no thread has been started.
    unsafe { gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "") };
    gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR).ok();
    gettextrs::bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8").ok();
    gettextrs::textdomain(config::GETTEXT_PACKAGE).ok();

    gio::resources_register_include!("chord.gresource").expect("the resources are built in");
    glib::set_application_name("Chord");
    application::ChordApplication::new().run()
}

/// `template` with each `%s` in it replaced by the next of `values`. Translated
/// messages hold their values this way; one pass, so a value holding `%s` stays whole.
pub fn fill(template: &str, values: &[&str]) -> String {
    let mut filled = String::with_capacity(template.len());
    let mut values = values.iter();
    let mut rest = template;
    while let Some(at) = rest.find("%s") {
        filled.push_str(&rest[..at]);
        filled.push_str(values.next().copied().unwrap_or_default());
        rest = &rest[at + 2..];
    }
    filled.push_str(rest);
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_fill_the_placeholders_in_order() {
        assert_eq!(fill("Ln %s, Col %s", &["3", "14"]), "Ln 3, Col 14");
    }

    #[test]
    fn a_value_holding_a_placeholder_is_not_filled_again() {
        assert_eq!(
            fill("Could not open “%s”: %s", &["100%s.txt", "denied"]),
            "Could not open “100%s.txt”: denied"
        );
    }
}
