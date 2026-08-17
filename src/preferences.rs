use adw::prelude::*;
use gtk::pango;

use crate::config;

/// The preferences, each row bound to its setting.
pub fn dialog(settings: &gio::Settings) -> adw::PreferencesDialog {
    let builder =
        gtk::Builder::from_resource(&format!("{}/ui/preferences.ui", config::RESOURCE_PATH));
    let object = |id: &str| -> glib::Object {
        builder
            .object(id)
            .unwrap_or_else(|| panic!("no {id} in the preferences"))
    };
    for (key, row) in [
        ("use-system-font", "use_system_font"),
        ("show-line-numbers", "show_line_numbers"),
        ("highlight-current-line", "highlight_current_line"),
        ("wrap-text", "wrap_text"),
        ("show-right-margin", "show_right_margin"),
        ("insert-spaces", "insert_spaces"),
        ("auto-indent", "auto_indent"),
    ] {
        settings.bind(key, &object(row), "active").build();
    }
    for (key, row) in [
        ("tab-width", "tab_width"),
        ("right-margin-position", "right_margin_position"),
    ] {
        settings.bind(key, &object(row), "value").build();
    }
    settings
        .bind("font", &object("font_button"), "font-desc")
        .mapping(|value, _| {
            let name = value.get::<String>()?;
            Some(pango::FontDescription::from_string(&name).to_value())
        })
        .set_mapping(|value, _| {
            let font = value.get::<pango::FontDescription>().ok()?;
            Some(font.to_str().to_variant())
        })
        .build();
    // Bound here rather than in the interface, where the row comes out sensitive whatever
    // the switch says until it is flipped.
    object("show_right_margin")
        .bind_property("active", &object("right_margin_position"), "sensitive")
        .sync_create()
        .build();
    object("preferences")
        .downcast()
        .expect("the preferences are a preferences dialog")
}
