//! The text view, which can shade every other line.

use std::cell::Cell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use sourceview5::subclass::prelude::*;

/// How much of the text's colour shades a line: enough to follow a row, too little to
/// read as a highlight.
const SHADE_ALPHA: f32 = 0.025;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::ChordView)]
    pub struct ChordView {
        #[property(get, set = Self::set_shade_alternate_lines)]
        shade_alternate_lines: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChordView {
        const NAME: &'static str = "ChordView";
        type Type = super::ChordView;
        type ParentType = sourceview5::View;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ChordView {}

    impl WidgetImpl for ChordView {}

    impl TextViewImpl for ChordView {
        fn snapshot_layer(&self, layer: gtk::TextViewLayer, snapshot: gtk::Snapshot) {
            // Below GtkSourceView's own layer, so the current line's highlight covers
            // the shade.
            if layer == gtk::TextViewLayer::BelowText && self.shade_alternate_lines.get() {
                self.obj().shade_lines(&snapshot);
            }
            self.parent_snapshot_layer(layer, snapshot);
        }
    }

    impl ViewImpl for ChordView {}

    impl ChordView {
        fn set_shade_alternate_lines(&self, shade: bool) {
            self.shade_alternate_lines.set(shade);
            self.obj().queue_draw();
        }
    }
}

glib::wrapper! {
    pub struct ChordView(ObjectSubclass<imp::ChordView>)
        @extends sourceview5::View, gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl ChordView {
    /// Shades the second, fourth… lines on screen, a wrapped line as a whole. The layer
    /// is drawn in buffer coordinates.
    fn shade_lines(&self, snapshot: &gtk::Snapshot) {
        let visible = self.visible_rect();
        let color = self.color();
        let shade = gtk::gdk::RGBA::new(color.red(), color.green(), color.blue(), SHADE_ALPHA);
        let bottom = visible.y() + visible.height();
        let (mut iter, _) = self.line_at_y(visible.y());
        loop {
            let (y, height) = self.line_yrange(&iter);
            if y >= bottom {
                break;
            }
            if iter.line() % 2 == 1 {
                snapshot.append_color(
                    &shade,
                    &gtk::graphene::Rect::new(
                        visible.x() as f32,
                        y as f32,
                        visible.width() as f32,
                        height as f32,
                    ),
                );
            }
            if !iter.forward_line() {
                break;
            }
        }
    }
}
