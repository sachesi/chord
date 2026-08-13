use std::{
    cell::{Cell, OnceCell},
    ops::ControlFlow,
    os::unix::ffi::OsStrExt,
};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::pango;

use crate::{config, font, window::ChordWindow};

mod imp {
    use super::*;

    pub struct ChordApplication {
        pub settings: gio::Settings,
        /// The font of the text, see [`font::css`]; made at startup, once GTK is.
        pub font: OnceCell<gtk::CssProvider>,
        pub zoom: Cell<f64>,
    }

    impl Default for ChordApplication {
        fn default() -> Self {
            Self {
                settings: gio::Settings::new(config::APP_ID),
                font: OnceCell::new(),
                zoom: Cell::new(1.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChordApplication {
        const NAME: &'static str = "ChordApplication";
        type Type = super::ChordApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for ChordApplication {}

    impl ApplicationImpl for ChordApplication {
        fn startup(&self) {
            self.parent_startup();
            sourceview5::init();
            let app = self.obj();
            app.setup_font();
            app.setup_actions();
        }

        /// D-Bus activation without files: the window last used, or a new one.
        fn activate(&self) {
            self.obj().present_window();
        }

        fn handle_local_options(&self, options: &glib::VariantDict) -> ControlFlow<glib::ExitCode> {
            if options.contains("version") {
                println!("chord {}", config::VERSION);
                return ControlFlow::Break(glib::ExitCode::SUCCESS);
            }
            ControlFlow::Continue(())
        }

        /// Every invocation reaches the running instance here.
        fn command_line(&self, command_line: &gio::ApplicationCommandLine) -> glib::ExitCode {
            let app = self.obj();
            let options = command_line.options_dict();
            let files: Vec<gio::File> = options
                .lookup_value("", Some(glib::VariantTy::BYTE_STRING_ARRAY))
                .and_then(|value| value.get::<Vec<Vec<u8>>>())
                .unwrap_or_default()
                .iter()
                .map(|arg| {
                    let arg = arg.strip_suffix(&[0]).unwrap_or(arg);
                    command_line.create_file_for_arg(std::ffi::OsStr::from_bytes(arg))
                })
                .collect();
            app.present_window().open_files(&files);
            glib::ExitCode::SUCCESS
        }
    }

    impl GtkApplicationImpl for ChordApplication {}
    impl AdwApplicationImpl for ChordApplication {}
}

/// The settings of the running application, shared by its windows and documents.
pub fn settings() -> gio::Settings {
    gio::Application::default()
        .and_downcast::<ChordApplication>()
        .expect("windows and documents live in the application")
        .imp()
        .settings
        .clone()
}

glib::wrapper! {
    pub struct ChordApplication(ObjectSubclass<imp::ChordApplication>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl ChordApplication {
    pub fn new() -> Self {
        let app: Self = glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("flags", gio::ApplicationFlags::HANDLES_COMMAND_LINE)
            .property("resource-base-path", config::RESOURCE_PATH)
            .build();
        app.add_main_option(
            "version",
            glib::Char::from(0),
            glib::OptionFlags::NONE,
            glib::OptionArg::None,
            &gettext("Print the version and exit"),
            None,
        );
        app.add_main_option(
            "",
            glib::Char::from(0),
            glib::OptionFlags::NONE,
            glib::OptionArg::FilenameArray,
            "",
            Some(&gettext("[FILE…]")),
        );
        app
    }

    /// A window with an empty document to start typing in.
    fn new_window(&self) -> ChordWindow {
        let window = ChordWindow::new(self);
        window.new_document();
        window.present();
        window
    }

    /// The window used last, or a new one.
    fn present_window(&self) -> ChordWindow {
        match self.active_window().and_downcast::<ChordWindow>() {
            Some(window) => {
                window.present();
                window
            }
            None => self.new_window(),
        }
    }

    fn setup_actions(&self) {
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| {
                // Each window asks about its own unsaved documents; the application ends
                // with the last of them.
                for window in app.windows() {
                    window.close();
                }
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about())
            .build();
        let zoom_in = gio::ActionEntry::builder("zoom-in")
            .activate(|app: &Self, _, _| app.zoom_by(0.1))
            .build();
        let zoom_out = gio::ActionEntry::builder("zoom-out")
            .activate(|app: &Self, _, _| app.zoom_by(-0.1))
            .build();
        let zoom_reset = gio::ActionEntry::builder("zoom-reset")
            .activate(|app: &Self, _, _| app.set_zoom(1.0))
            .build();
        self.add_action_entries([quit, about, zoom_in, zoom_out, zoom_reset]);

        let accels: [(&str, &[&str]); 14] = [
            ("app.quit", &["<Control>q"]),
            (
                "app.zoom-in",
                &["<Control>plus", "<Control>equal", "<Control>KP_Add"],
            ),
            ("app.zoom-out", &["<Control>minus", "<Control>KP_Subtract"]),
            ("app.zoom-reset", &["<Control>0", "<Control>KP_0"]),
            ("win.new-document", &["<Control>n", "<Control>t"]),
            ("win.open", &["<Control>o"]),
            ("win.save", &["<Control>s"]),
            ("win.save-as", &["<Control><Shift>s"]),
            ("win.close-document", &["<Control>w"]),
            ("window.close", &["<Control><Shift>w"]),
            ("win.find", &["<Control>f"]),
            ("win.replace", &["<Control>h"]),
            ("win.find-next", &["<Control>g"]),
            ("win.find-previous", &["<Control><Shift>g"]),
        ];
        for (action, keys) in accels {
            self.set_accels_for_action(action, keys);
        }
    }

    fn setup_font(&self) {
        let imp = self.imp();
        let provider = imp.font.get_or_init(gtk::CssProvider::new);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        adw::StyleManager::default().connect_monospace_font_name_notify(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_| app.update_font()
        ));
        self.update_font();
    }

    fn update_font(&self) {
        let imp = self.imp();
        let name = adw::StyleManager::default().monospace_font_name();
        let font = pango::FontDescription::from_string(&name);
        if let Some(provider) = imp.font.get() {
            provider.load_from_string(&font::css(&font, imp.zoom.get()));
        }
    }

    fn zoom_by(&self, step: f64) {
        self.set_zoom(self.imp().zoom.get() + step);
    }

    fn set_zoom(&self, zoom: f64) {
        // Tenths, so that steps up and down come back to where they started.
        let zoom = (zoom.clamp(font::MIN_ZOOM, font::MAX_ZOOM) * 10.0).round() / 10.0;
        self.imp().zoom.set(zoom);
        self.update_font();
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("Chord")
            .application_icon(config::APP_ID)
            .developer_name("sachesi")
            .version(config::VERSION)
            .website("https://github.com/sachesi/chord")
            .issue_url("https://github.com/sachesi/chord/issues")
            .license_type(gtk::License::Gpl30)
            .comments(gettext("Edit text files"))
            // Translators: put your name here, one per line, optionally with an email address.
            .translator_credits(gettext("translator-credits"))
            .build();
        about.present(self.active_window().as_ref());
    }
}
