//! One document: its text, the file it came from or goes to, and its search.

use std::{
    cell::{Cell, OnceCell, RefCell},
    path::Path,
};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use sourceview5::prelude::*;

use crate::{application, fill};

/// When a file was last changed, as GIO gives it: seconds, and microseconds within them.
type Stamp = (u64, u32);

const STAMP_ATTRIBUTES: &str = "time::modified,time::modified-usec";

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate, glib::Properties)]
    #[template(resource = "/io/github/sachesi/chord/ui/document.ui")]
    #[properties(wrapper_type = super::Document)]
    pub struct Document {
        #[template_child]
        pub banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub occurrences: TemplateChild<gtk::Label>,
        #[template_child]
        pub replace_toggle: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub case_sensitive: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub whole_words: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub regex: TemplateChild<gtk::CheckButton>,
        #[template_child]
        pub replace_entry: TemplateChild<gtk::Entry>,
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
        pub search: OnceCell<sourceview5::SearchContext>,
        /// Follows insert-spaces on the shared settings, which outlive the document.
        pub indentation_changed: RefCell<Option<glib::SignalHandlerId>>,
        /// When the file last changed on disk, as of its last load or save here.
        pub on_disk: Cell<Option<Stamp>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Document {
        const NAME: &'static str = "ChordDocument";
        type Type = super::Document;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            sourceview5::View::ensure_type();
            klass.bind_template();
            klass.bind_template_callbacks();
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
            if let Some(handler) = self.indentation_changed.take() {
                application::settings().disconnect(handler);
            }
            self.dispose_template();
        }
    }

    impl WidgetImpl for Document {
        fn grab_focus(&self) -> bool {
            self.view.grab_focus()
        }
    }

    impl BinImpl for Document {}

    #[gtk::template_callbacks]
    impl Document {
        #[template_callback]
        fn on_reload(&self) {
            self.obj().reload();
        }

        #[template_callback]
        fn find_next(&self) {
            self.obj().find_next();
        }

        #[template_callback]
        fn find_previous(&self) {
            self.obj().find_previous();
        }

        #[template_callback]
        fn close_search(&self) {
            self.search_bar.set_search_mode(false);
        }

        #[template_callback]
        fn replace_one(&self) {
            self.obj().replace_one();
        }

        #[template_callback]
        fn replace_all(&self) {
            self.obj().replace_all();
        }
    }
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

    /// The line the cursor is on, counted from 1.
    fn line(&self) -> u32 {
        let buffer = self.buffer();
        let line = buffer.iter_at_mark(&buffer.get_insert()).line();
        u32::try_from(line).unwrap_or(0) + 1
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
        self.setup_settings();
        self.setup_search();
        self.update_location();
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let settings = application::settings();
        let view = &*imp.view;
        // Keys named after the properties of the view they set.
        for key in [
            "show-line-numbers",
            "highlight-current-line",
            "show-right-margin",
            "right-margin-position",
            "tab-width",
            "auto-indent",
        ] {
            settings.bind(key, view, key).get().build();
        }
        settings
            .bind("wrap-text", view, "wrap-mode")
            .get()
            .mapping(|value, _| {
                let mode = if value.get::<bool>()? {
                    gtk::WrapMode::WordChar
                } else {
                    gtk::WrapMode::None
                };
                Some(mode.to_value())
            })
            .build();
        let handler = settings.connect_changed(
            Some("insert-spaces"),
            glib::clone!(
                #[weak(rename_to = document)]
                self,
                move |_, _| document.update_indentation()
            ),
        );
        imp.indentation_changed.replace(Some(handler));
        self.update_indentation();
    }

    fn update_indentation(&self) {
        let makefile = self
            .buffer()
            .language()
            .is_some_and(|language| language.id() == "makefile");
        self.imp().view.set_insert_spaces_instead_of_tabs(
            application::settings().boolean("insert-spaces") && !makefile,
        );
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
    /// document empty, to be made when it is saved. The document takes the location and
    /// turns busy at once, before the future is first polled, so nothing else is opened
    /// into it meanwhile.
    pub fn load(
        &self,
        location: &gio::File,
    ) -> impl Future<Output = Result<(), glib::Error>> + use<> {
        let imp = self.imp();
        imp.file.set_location(Some(location));
        self.update_location();
        imp.banner.set_revealed(false);
        self.set_busy(true);
        let loader = sourceview5::FileLoader::new(&self.buffer(), &imp.file);
        let (loaded, _) = loader.load_future(glib::Priority::DEFAULT);
        let document = self.clone();
        let location = location.clone();
        async move {
            let result = loaded.await;
            document.set_busy(false);
            match result {
                Ok(()) => {}
                Err(error) if error.matches(gio::IOErrorEnum::NotFound) => {}
                Err(error) if error.matches(sourceview5::FileLoaderError::ConversionFallback) => {
                    document.show_notice(
                        &gettext(
                            "Parts of this file are not text and show as codes; saving writes the codes",
                        ),
                        false,
                    );
                }
                Err(error) => return Err(error),
            }
            let buffer = document.buffer();
            buffer.set_modified(false);
            buffer.place_cursor(&buffer.start_iter());
            document.guess_language();
            document.remember_disk_state().await;
            remember_recent(&location);
            Ok(())
        }
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
        self.remember_disk_state().await;
        if let Some(location) = self.location() {
            remember_recent(&location);
        }
        Ok(())
    }

    fn reload(&self) {
        let Some(location) = self.location() else {
            return;
        };
        let line = self.line();
        let loaded = self.load(&location);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = document)]
            self,
            async move {
                match loaded.await {
                    Ok(()) => document.go_to(line),
                    Err(error) => document.show_notice(error.message(), false),
                }
            }
        ));
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
        self.update_indentation();
    }

    async fn remember_disk_state(&self) {
        let stamp = match self.location() {
            Some(location) => stamp(&location).await.ok().flatten(),
            None => None,
        };
        self.imp().on_disk.set(stamp);
    }

    /// Says so in the banner when the file changed or went away since it was loaded or
    /// saved here.
    pub fn check_on_disk(&self) {
        let (Some(location), Some(known)) = (self.location(), self.imp().on_disk.get()) else {
            return;
        };
        if self.busy() {
            return;
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = document)]
            self,
            async move {
                let Ok(now) = stamp(&location).await else {
                    return;
                };
                // A load or a save meanwhile has a newer answer.
                if document.busy() || document.imp().on_disk.get() != Some(known) {
                    return;
                }
                match now {
                    Some(now) if now == known => {}
                    Some(_) if document.is_modified() => document.show_notice(
                        &gettext("The file changed on disk; reloading discards your changes"),
                        true,
                    ),
                    Some(_) => document.show_notice(&gettext("The file changed on disk"), true),
                    None => document.show_notice(
                        &gettext("The file is no longer on disk; saving writes it again"),
                        false,
                    ),
                }
            }
        ));
    }

    fn show_notice(&self, title: &str, offer_reload: bool) {
        let banner = &self.imp().banner;
        banner.set_title(title);
        banner.set_button_label(offer_reload.then(|| gettext("_Reload")).as_deref());
        banner.set_revealed(true);
    }

    /// Puts the cursor at the start of `line`, counted from 1 and kept within the text.
    fn go_to(&self, line: u32) {
        let buffer = self.buffer();
        let line = i32::try_from(line.saturating_sub(1))
            .unwrap_or(i32::MAX)
            .min(buffer.line_count() - 1);
        let Some(iter) = buffer.iter_at_line(line) else {
            return;
        };
        buffer.place_cursor(&iter);
        let view = &self.imp().view;
        view.scroll_to_mark(&buffer.get_insert(), 0.0, true, 0.0, 0.5);
        view.grab_focus();
    }

    fn setup_search(&self) {
        let imp = self.imp();
        let settings = sourceview5::SearchSettings::new();
        settings.set_wrap_around(true);
        imp.search_entry
            .bind_property("text", &settings, "search-text")
            .sync_create()
            .build();
        imp.case_sensitive
            .bind_property("active", &settings, "case-sensitive")
            .sync_create()
            .build();
        imp.whole_words
            .bind_property("active", &settings, "at-word-boundaries")
            .sync_create()
            .build();
        imp.regex
            .bind_property("active", &settings, "regex-enabled")
            .sync_create()
            .build();

        let buffer = self.buffer();
        let search = sourceview5::SearchContext::new(&buffer, Some(&settings));
        imp.search_bar
            .bind_property("search-mode-enabled", &search, "highlight")
            .sync_create()
            .build();
        search.connect_occurrences_count_notify(glib::clone!(
            #[weak(rename_to = document)]
            self,
            move |_| document.update_occurrences()
        ));
        search.connect_regex_error_notify(glib::clone!(
            #[weak(rename_to = document)]
            self,
            move |search| {
                let entry = &document.imp().search_entry;
                if search.regex_error().is_some() {
                    entry.add_css_class("error");
                } else {
                    entry.remove_css_class("error");
                }
            }
        ));
        buffer.connect_mark_set(glib::clone!(
            #[weak(rename_to = document)]
            self,
            move |buffer, _, mark| {
                if *mark == buffer.get_insert() {
                    document.update_occurrences();
                }
            }
        ));
        imp.search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = document)]
            self,
            move |_| {
                let buffer = document.buffer();
                let from = buffer.selection_bounds().map_or_else(
                    || buffer.iter_at_mark(&buffer.get_insert()),
                    |(start, _)| start,
                );
                document.select_match(document.search().forward(&from));
            }
        ));
        imp.search_bar
            .connect_search_mode_enabled_notify(glib::clone!(
                #[weak(rename_to = document)]
                self,
                move |bar| {
                    if bar.is_search_mode() {
                        document.update_occurrences();
                    } else {
                        document.imp().view.grab_focus();
                    }
                }
            ));
        // Escape closes the search from the text and the buttons too, not only from its
        // entry.
        let escape = gtk::ShortcutController::new();
        escape.add_shortcut(gtk::Shortcut::new(
            Some(gtk::KeyvalTrigger::new(
                gtk::gdk::Key::Escape,
                gtk::gdk::ModifierType::empty(),
            )),
            Some(gtk::CallbackAction::new(|widget, _| {
                let Some(document) = widget.downcast_ref::<Self>() else {
                    return glib::Propagation::Proceed;
                };
                let bar = &document.imp().search_bar;
                if !bar.is_search_mode() {
                    return glib::Propagation::Proceed;
                }
                bar.set_search_mode(false);
                glib::Propagation::Stop
            })),
        ));
        self.add_controller(escape);
        imp.search.set(search).ok();
    }

    fn search(&self) -> &sourceview5::SearchContext {
        self.imp()
            .search
            .get()
            .expect("the search is set up with the document")
    }

    /// Opens the search, with what is selected on one line as the text to find.
    pub fn show_search(&self, replace: bool) {
        let imp = self.imp();
        let buffer = self.buffer();
        if let Some((start, end)) = buffer.selection_bounds()
            && start.line() == end.line()
        {
            imp.search_entry.set_text(&buffer.text(&start, &end, false));
        }
        imp.replace_toggle.set_active(replace);
        imp.search_bar.set_search_mode(true);
        imp.search_entry.grab_focus();
        imp.search_entry.select_region(0, -1);
    }

    pub fn find_next(&self) {
        let buffer = self.buffer();
        let from = buffer
            .selection_bounds()
            .map_or_else(|| buffer.iter_at_mark(&buffer.get_insert()), |(_, end)| end);
        self.select_match(self.search().forward(&from));
    }

    pub fn find_previous(&self) {
        let buffer = self.buffer();
        let from = buffer.selection_bounds().map_or_else(
            || buffer.iter_at_mark(&buffer.get_insert()),
            |(start, _)| start,
        );
        self.select_match(self.search().backward(&from));
    }

    fn select_match(&self, found: Option<(gtk::TextIter, gtk::TextIter, bool)>) {
        if let Some((start, end, _)) = found {
            let buffer = self.buffer();
            buffer.select_range(&start, &end);
            self.imp()
                .view
                .scroll_to_mark(&buffer.get_insert(), 0.25, false, 0.0, 0.0);
        }
    }

    fn replace_one(&self) {
        let search = self.search();
        let buffer = self.buffer();
        // The bindings assert on a replacement GtkSourceView declines without an error,
        // which it does for anything but a match it has counted.
        if let Some((mut start, mut end)) = buffer.selection_bounds()
            && search.occurrence_position(&start, &end) > 0
            && search
                .replace(&mut start, &mut end, &self.imp().replace_entry.text())
                .is_ok()
        {
            buffer.place_cursor(&end);
        }
        self.find_next();
    }

    fn replace_all(&self) {
        let search = self.search();
        // The bindings assert that nothing replaced is an error. The count stays unknown
        // until the whole text has been searched, which takes a while in a large file, so
        // until then one match found from the start stands for it.
        let found = match search.occurrences_count() {
            0 => false,
            count if count > 0 => true,
            _ => search.forward(&self.buffer().start_iter()).is_some(),
        };
        if found {
            let _ = search.replace_all(&self.imp().replace_entry.text());
        }
    }

    fn update_occurrences(&self) {
        let imp = self.imp();
        let Some(search) = imp.search.get() else {
            return;
        };
        if !imp.search_bar.is_search_mode() {
            return;
        }
        let count = search.occurrences_count();
        let label = if imp.search_entry.text().is_empty() || count < 0 {
            String::new()
        } else if count == 0 {
            gettext("No results")
        } else {
            let position = self
                .buffer()
                .selection_bounds()
                .map_or(0, |(start, end)| search.occurrence_position(&start, &end));
            if position > 0 {
                fill(
                    &gettext("%s of %s"),
                    &[&position.to_string(), &count.to_string()],
                )
            } else {
                fill(
                    &ngettext("%s result", "%s results", count.unsigned_abs()),
                    &[&count.to_string()],
                )
            }
        };
        imp.occurrences.set_label(&label);
    }
}

async fn stamp(location: &gio::File) -> Result<Option<Stamp>, glib::Error> {
    match location
        .query_info_future(
            STAMP_ATTRIBUTES,
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await
    {
        Ok(info) => Ok(Some((
            info.attribute_uint64("time::modified"),
            info.attribute_uint32("time::modified-usec"),
        ))),
        Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(None),
        Err(error) => Err(error),
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
