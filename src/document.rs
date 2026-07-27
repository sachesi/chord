//! One document: its text and the file it came from or goes to.

use std::{
    cell::{Cell, RefCell},
    path::Path,
};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use sourceview5::prelude::*;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate, glib::Properties)]
    #[template(resource = "/io/github/sachesi/chord/ui/document.ui")]
    #[properties(wrapper_type = super::Document)]
    pub struct Document {
        #[template_child]
        pub banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub view: TemplateChild<sourceview5::View>,
        /// The name, marked while there are changes not saved.
        #[property(get)]
        pub title: RefCell<String>,
        /// The folder the file is in, shortened, for the window's subtitle.
        #[property(get)]
        pub folder: RefCell<String>,
        /// The whole location, for the tab's tooltip.
        #[property(get)]
        pub location_label: RefCell<String>,
        /// Loading or saving; the text cannot be edited meanwhile.
        #[property(get)]
        pub busy: Cell<bool>,
        pub file: sourceview5::File,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Document {
        const NAME: &'static str = "ChordDocument";
        type Type = super::Document;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            sourceview5::View::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Document {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for Document {
        fn grab_focus(&self) -> bool {
            self.view.grab_focus()
        }
    }

    impl BinImpl for Document {}
}

glib::wrapper! {
    pub struct Document(ObjectSubclass<imp::Document>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Document {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Document {
    pub fn buffer(&self) -> sourceview5::Buffer {
        self.imp()
            .view
            .buffer()
            .downcast()
            .expect("a source view has a source buffer")
    }

    pub fn location(&self) -> Option<gio::File> {
        self.imp().file.location()
    }

    pub fn is_modified(&self) -> bool {
        self.buffer().is_modified()
    }

    /// Nothing typed and nothing opened: a file can be opened into it instead of a new tab.
    pub fn is_blank(&self) -> bool {
        self.location().is_none() && !self.busy() && self.buffer().char_count() == 0
    }

    /// The file's name, or what a document not saved yet is called.
    pub fn name(&self) -> String {
        self.location()
            .and_then(|location| location.basename())
            .map_or_else(
                || gettext("Untitled Document"),
                |name| name.to_string_lossy().into_owned(),
            )
    }

    fn setup(&self) {
        let buffer = self.buffer();
        adw::StyleManager::default()
            .bind_property("dark", &buffer, "style-scheme")
            .transform_to(|_, dark: bool| {
                sourceview5::StyleSchemeManager::default().scheme(if dark {
                    "Adwaita-dark"
                } else {
                    "Adwaita"
                })
            })
            .sync_create()
            .build();
        buffer.connect_modified_changed(glib::clone!(
            #[weak(rename_to = document)]
            self,
            move |_| document.update_title()
        ));
        self.update_location();
    }

    fn update_title(&self) {
        let name = self.name();
        let title = if self.is_modified() {
            format!("• {name}")
        } else {
            name
        };
        self.imp().title.replace(title);
        self.notify_title();
    }

    fn update_location(&self) {
        let imp = self.imp();
        let (folder, label) = self.location().map_or_else(Default::default, |location| {
            let folder = location
                .parent()
                .map(|parent| match parent.path() {
                    Some(path) => shorten_home(&path, &glib::home_dir()),
                    None => parent.parse_name().into(),
                })
                .unwrap_or_default();
            (folder, location.parse_name().into())
        });
        imp.folder.replace(folder);
        imp.location_label.replace(label);
        self.notify_folder();
        self.notify_location_label();
        self.update_title();
    }

    fn set_busy(&self, busy: bool) {
        let imp = self.imp();
        imp.busy.set(busy);
        imp.view.set_editable(!busy);
        self.notify_busy();
    }

    /// Reads `location` into the document. A file that does not exist yet leaves the
    /// document empty, to be made when it is saved.
    pub async fn load(&self, location: &gio::File) -> Result<(), glib::Error> {
        let imp = self.imp();
        imp.file.set_location(Some(location));
        self.update_location();
        imp.banner.set_revealed(false);
        self.set_busy(true);
        let loader = sourceview5::FileLoader::new(&self.buffer(), &imp.file);
        let result = loader.load_future(glib::Priority::DEFAULT).0.await;
        self.set_busy(false);
        let buffer = self.buffer();
        buffer.set_modified(false);
        match result {
            Ok(()) => {}
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => {}
            Err(error) if error.matches(sourceview5::FileLoaderError::ConversionFallback) => {
                self.show_notice(&gettext(
                    "Parts of this file are not text and show as codes; saving writes the codes",
                ));
            }
            Err(error) => return Err(error),
        }
        buffer.place_cursor(&buffer.start_iter());
        self.guess_language();
        remember_recent(location);
        Ok(())
    }

    /// Writes the document to its file, or to `target` when given, which then becomes its
    /// file. `flags` let it save over what [`sourceview5::FileSaver`] would refuse to.
    pub async fn save(
        &self,
        target: Option<&gio::File>,
        flags: sourceview5::FileSaverFlags,
    ) -> Result<(), glib::Error> {
        let imp = self.imp();
        let buffer = self.buffer();
        let saver = match target {
            Some(target) => sourceview5::FileSaver::with_target(&buffer, &imp.file, target),
            None => sourceview5::FileSaver::new(&buffer, &imp.file),
        };
        saver.set_flags(flags);
        self.set_busy(true);
        let result = saver.save_future(glib::Priority::DEFAULT).0.await;
        self.set_busy(false);
        result?;
        if target.is_some() {
            self.update_location();
            self.guess_language();
        }
        imp.banner.set_revealed(false);
        if let Some(location) = self.location() {
            remember_recent(&location);
        }
        Ok(())
    }

    fn guess_language(&self) {
        let buffer = self.buffer();
        let name = self
            .location()
            .and_then(|location| location.basename())
            .map(|name| name.to_string_lossy().into_owned());
        let start = buffer.start_iter();
        let mut end = start;
        end.forward_chars(4096);
        let head = buffer.text(&start, &end, false);
        let (content_type, _) = gio::content_type_guess(name.as_deref(), head.as_bytes());
        let language = sourceview5::LanguageManager::default()
            .guess_language(name.as_deref(), Some(&content_type));
        buffer.set_language(language.as_ref());
    }

    fn show_notice(&self, title: &str) {
        let banner = &self.imp().banner;
        banner.set_title(title);
        banner.set_revealed(true);
    }
}

fn remember_recent(location: &gio::File) {
    gtk::RecentManager::default().add_item(&location.uri());
}

/// `path` with the home folder written as `~`.
fn shorten_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_folder_reads_as_a_tilde() {
        let home = Path::new("/home/me");
        assert_eq!(shorten_home(Path::new("/home/me"), home), "~");
        assert_eq!(shorten_home(Path::new("/home/me/notes"), home), "~/notes");
        assert_eq!(
            shorten_home(Path::new("/home/meadow"), home),
            "/home/meadow"
        );
        assert_eq!(shorten_home(Path::new("/etc"), home), "/etc");
    }
}
