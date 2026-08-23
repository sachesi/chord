//! A window of documents, one to a tab.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};

use crate::{
    application::{self, ChordApplication},
    document::Document,
    fill,
};

/// The actions that act on the document shown, off while there is none.
const DOCUMENT_ACTIONS: [&str; 7] = [
    "win.save",
    "win.save-as",
    "win.close-document",
    "win.find",
    "win.replace",
    "win.find-next",
    "win.find-previous",
];

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/sachesi/chord/ui/window.ui")]
    pub struct ChordWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub tab_view: TemplateChild<adw::TabView>,
        /// What ties the header to the document shown, undone when another is shown.
        pub shown: RefCell<Vec<glib::Binding>>,
        /// Set once the documents not saved have been settled, so the window may close.
        pub closing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChordWindow {
        const NAME: &'static str = "ChordWindow";
        type Type = super::ChordWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.install_action("win.new-document", None, |window, _, _| {
                window.new_document();
            });
            klass.install_action("win.open", None, |window, _, _| window.choose_files());
            klass.install_action("win.save", None, |window, _, _| window.save_shown(false));
            klass.install_action("win.save-as", None, |window, _, _| window.save_shown(true));
            klass.install_action("win.close-document", None, |window, _, _| {
                let tab_view = &window.imp().tab_view;
                if let Some(page) = tab_view.selected_page() {
                    tab_view.close_page(&page);
                }
            });
            klass.install_action("win.find", None, |window, _, _| {
                if let Some(document) = window.document() {
                    document.show_search(false);
                }
            });
            klass.install_action("win.replace", None, |window, _, _| {
                if let Some(document) = window.document() {
                    document.show_search(true);
                }
            });
            klass.install_action("win.find-next", None, |window, _, _| {
                if let Some(document) = window.document() {
                    document.find_next();
                }
            });
            klass.install_action("win.find-previous", None, |window, _, _| {
                if let Some(document) = window.document() {
                    document.find_previous();
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ChordWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for ChordWindow {}

    impl WindowImpl for ChordWindow {
        fn close_request(&self) -> glib::Propagation {
            self.obj().close_request()
        }
    }

    impl ApplicationWindowImpl for ChordWindow {}
    impl AdwApplicationWindowImpl for ChordWindow {}
}

glib::wrapper! {
    pub struct ChordWindow(ObjectSubclass<imp::ChordWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl ChordWindow {
    /// A window without documents.
    pub fn new(app: &ChordApplication) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn document(&self) -> Option<Document> {
        self.imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast().ok())
    }

    fn documents(&self) -> Vec<Document> {
        let tab_view = &self.imp().tab_view;
        (0..tab_view.n_pages())
            .filter_map(|index| tab_view.nth_page(index).child().downcast().ok())
            .collect()
    }

    pub fn page_of(&self, file: &gio::File) -> Option<adw::TabPage> {
        let tab_view = &self.imp().tab_view;
        (0..tab_view.n_pages())
            .map(|index| tab_view.nth_page(index))
            .find(|page| {
                page.child()
                    .downcast::<Document>()
                    .ok()
                    .and_then(|document| document.location())
                    .is_some_and(|location| location.equal(file))
            })
    }

    fn setup(&self) {
        let imp = self.imp();
        let settings = application::settings();
        self.set_default_size(settings.int("window-width"), settings.int("window-height"));
        if settings.boolean("window-maximized") {
            self.maximize();
        }

        let tab_view = &*imp.tab_view;
        // Also for tabs dragged in from another window, which come as new pages.
        tab_view.connect_page_attached(|_, page, _| {
            let Ok(document) = page.child().downcast::<Document>() else {
                return;
            };
            document
                .bind_property("title", page, "title")
                .sync_create()
                .build();
            document
                .bind_property("location-label", page, "tooltip")
                .sync_create()
                .build();
            document
                .bind_property("busy", page, "loading")
                .sync_create()
                .build();
        });
        tab_view.connect_selected_page_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| window.show_document()
        ));
        tab_view.connect_n_pages_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |tab_view| {
                let page = if tab_view.n_pages() == 0 {
                    "empty"
                } else {
                    "documents"
                };
                window.imp().stack.set_visible_child_name(page);
            }
        ));
        tab_view.connect_close_page(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |tab_view, page| window.close_page(tab_view, page)
        ));
        tab_view.connect_create_window(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[upgrade_or]
            None,
            move |_| {
                let app = window.application().and_downcast::<ChordApplication>()?;
                let new = Self::new(&app);
                new.present();
                Some(new.imp().tab_view.get())
            }
        ));

        self.connect_is_active_notify(|window| {
            if window.is_active()
                && let Some(document) = window.document()
            {
                document.check_on_disk();
            }
        });

        self.show_document();
    }

    /// Points the header and the actions at the document now shown.
    fn show_document(&self) {
        let imp = self.imp();
        for binding in imp.shown.take() {
            binding.unbind();
        }
        let document = self.document();
        for action in DOCUMENT_ACTIONS {
            self.action_set_enabled(action, document.is_some());
        }
        let Some(document) = document else {
            let name = gettext("Chord");
            imp.window_title.set_title(&name);
            imp.window_title.set_subtitle("");
            self.set_title(Some(&name));
            return;
        };
        imp.shown.replace(vec![
            document
                .bind_property("title", &*imp.window_title, "title")
                .sync_create()
                .build(),
            document
                .bind_property("folder", &*imp.window_title, "subtitle")
                .sync_create()
                .build(),
            document
                .bind_property("title", self, "title")
                .sync_create()
                .build(),
        ]);
        document.check_on_disk();
    }

    pub fn new_document(&self) -> Document {
        let document = Document::default();
        let tab_view = &self.imp().tab_view;
        let page = tab_view.append(&document);
        tab_view.set_selected_page(&page);
        document.grab_focus();
        document
    }

    fn select(&self, document: &Document) {
        let tab_view = &self.imp().tab_view;
        tab_view.set_selected_page(&tab_view.page(document));
    }

    /// Opens each of `files` in a tab of its own; one open already, in this window or
    /// another, is shown instead. An empty document on screen takes the first.
    pub fn open_files(&self, files: &[gio::File]) {
        let app = self.application().and_downcast::<ChordApplication>();
        for file in files {
            if let Some((window, page)) = app.as_ref().and_then(|app| app.find_document(file)) {
                window.imp().tab_view.set_selected_page(&page);
                window.present();
                continue;
            }
            let document = self
                .document()
                .filter(Document::is_blank)
                .unwrap_or_else(|| self.new_document());
            self.load(&document, file);
        }
    }

    fn load(&self, document: &Document, file: &gio::File) {
        let loaded = document.load(file);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            document,
            async move {
                let Err(error) = loaded.await else {
                    return;
                };
                window.toast(&fill(
                    &gettext("Could not open “%s”: %s"),
                    &[&document.name(), error.message()],
                ));
                // From wherever the tab is by now: it can be dragged away while it loads.
                if let Some(tab_view) = document
                    .ancestor(adw::TabView::static_type())
                    .and_downcast::<adw::TabView>()
                {
                    tab_view.close_page(&tab_view.page(&document));
                }
            }
        ));
    }

    fn choose_files(&self) {
        let text = gtk::FileFilter::new();
        text.set_name(Some(&gettext("Text Files")));
        text.add_mime_type("text/plain");
        let all = gtk::FileFilter::new();
        all.set_name(Some(&gettext("All Files")));
        all.add_pattern("*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&text);
        filters.append(&all);
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open Files"))
            .modal(true)
            .filters(&filters)
            .default_filter(&text)
            .build();
        if let Some(folder) = self
            .document()
            .and_then(|document| document.location())
            .and_then(|location| location.parent())
        {
            dialog.set_initial_folder(Some(&folder));
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                let Ok(files) = dialog.open_multiple_future(Some(&window)).await else {
                    return;
                };
                let files: Vec<gio::File> =
                    files.iter::<gio::File>().filter_map(Result::ok).collect();
                window.open_files(&files);
            }
        ));
    }

    fn save_shown(&self, choose: bool) {
        let Some(document) = self.document() else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                window.save(&document, choose).await;
            }
        ));
    }

    /// Saves `document`, asking where to for one never saved or when `choose`. False
    /// when it was not saved.
    async fn save(&self, document: &Document, choose: bool) -> bool {
        if document.busy() {
            return false;
        }
        let target = if choose || document.location().is_none() {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save As"))
                .modal(true)
                .build();
            match document.location() {
                Some(location) => dialog.set_initial_file(Some(&location)),
                None => dialog.set_initial_name(Some(&document.name())),
            }
            match dialog.save_future(Some(self)).await {
                Ok(file) => Some(file),
                Err(_) => return false,
            }
        } else {
            None
        };
        let name = target.as_ref().and_then(gio::File::basename).map_or_else(
            || document.name(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut flags = sourceview5::FileSaverFlags::empty();
        loop {
            let Err(error) = document.save(target.as_ref(), flags).await else {
                return true;
            };
            let (flag, heading, body, accept) = if error
                .matches(sourceview5::FileSaverError::ExternallyModified)
            {
                (
                    sourceview5::FileSaverFlags::IGNORE_MODIFICATION_TIME,
                    gettext("Replace the File on Disk?"),
                    gettext(
                        "“%s” was changed by another program after it was opened here. Saving replaces those changes.",
                    ),
                    gettext("_Replace"),
                )
            } else if error.matches(sourceview5::FileSaverError::InvalidChars) {
                (
                    sourceview5::FileSaverFlags::IGNORE_INVALID_CHARS,
                    gettext("Save the Codes?"),
                    gettext(
                        "Parts of “%s” could not be read as text. Saving writes the codes shown in their place.",
                    ),
                    gettext("_Save"),
                )
            } else {
                self.toast(&fill(
                    &gettext("Could not save “%s”: %s"),
                    &[&name, error.message()],
                ));
                return false;
            };
            if flags.contains(flag)
                || !self
                    .confirm(&heading, &fill(&body, &[&name]), &accept)
                    .await
            {
                return false;
            }
            flags |= flag;
        }
    }

    async fn confirm(&self, heading: &str, body: &str, accept: &str) -> bool {
        let dialog = adw::AlertDialog::new(Some(heading), Some(body));
        dialog.add_responses(&[("cancel", &gettext("_Cancel")), ("accept", accept)]);
        dialog.set_response_appearance("accept", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.choose_future(Some(self)).await == "accept"
    }

    fn close_page(&self, tab_view: &adw::TabView, page: &adw::TabPage) -> glib::Propagation {
        let Ok(document) = page.child().downcast::<Document>() else {
            return glib::Propagation::Proceed;
        };
        if !document.is_modified() || self.imp().closing.get() {
            return glib::Propagation::Proceed;
        }
        tab_view.set_selected_page(page);
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            tab_view,
            #[weak]
            page,
            async move {
                let close = window.settle(&[document]).await;
                tab_view.close_page_finish(&page, close);
            }
        ));
        glib::Propagation::Stop
    }

    fn close_request(&self) -> glib::Propagation {
        let unsaved: Vec<Document> = self
            .documents()
            .into_iter()
            .filter(Document::is_modified)
            .collect();
        if self.imp().closing.get() || unsaved.is_empty() {
            self.remember_size();
            return glib::Propagation::Proceed;
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = window)]
            self,
            async move {
                if window.settle(&unsaved).await {
                    window.imp().closing.set(true);
                    window.close();
                }
            }
        ));
        glib::Propagation::Stop
    }

    /// Asks what becomes of the changes to `unsaved` before they close: true once they
    /// are saved or given up.
    async fn settle(&self, unsaved: &[Document]) -> bool {
        let body = match unsaved {
            [document] => {
                self.select(document);
                fill(
                    &gettext(
                        "“%s” has changes that are not saved. Closing without saving loses them.",
                    ),
                    &[&document.name()],
                )
            }
            _ => fill(
                &ngettext(
                    "%s document has changes that are not saved. Closing without saving loses them.",
                    "%s documents have changes that are not saved. Closing without saving loses them.",
                    u32::try_from(unsaved.len()).unwrap_or(u32::MAX),
                ),
                &[&unsaved.len().to_string()],
            ),
        };
        let (discard, save) = if unsaved.len() == 1 {
            (gettext("_Discard"), gettext("_Save"))
        } else {
            (gettext("_Discard All"), gettext("_Save All"))
        };
        let dialog = adw::AlertDialog::new(Some(&gettext("Save Changes?")), Some(&body));
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("discard", &discard),
            ("save", &save),
        ]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        match dialog.choose_future(Some(self)).await.as_str() {
            "save" => {
                for document in unsaved {
                    self.select(document);
                    if !self.save(document, false).await {
                        return false;
                    }
                }
                true
            }
            "discard" => true,
            _ => false,
        }
    }

    fn remember_size(&self) {
        let settings = application::settings();
        let (width, height) = self.default_size();
        let _ = settings.set_int("window-width", width);
        let _ = settings.set_int("window-height", height);
        let _ = settings.set_boolean("window-maximized", self.is_maximized());
    }

    fn toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        toast.set_use_markup(false);
        self.imp().toast_overlay.add_toast(toast);
    }
}
