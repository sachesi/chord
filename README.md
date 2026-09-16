# Chord

Chord is a text editor written in Rust with GTK 4, libadwaita and GtkSourceView 5. It
opens files in tabs, colours the syntax of the languages GtkSourceView knows, and keeps a
file's encoding and line endings as it found them. It runs on GTK 4.22, libadwaita 1.9 and
GtkSourceView 5 or newer.

Find and replace works as you type, with whole words, case and regular expressions as
options; Ctrl+L goes to a line, or a line and a column written as `12:4`. A file that
changes on disk while it is open says so above the text and offers to reload it, and
saving over such a change asks first. A file named on the command line that does not
exist yet opens empty and is made when it is saved. Makefiles are indented with tabs
whatever the setting for spaces says. The text is in the system's monospace font or one
chosen in the preferences, follows the system's light or dark style, and zooms with Ctrl++
and Ctrl+-.

Running `chord FILE…` again opens the files in the window used last, or in a new one with
`--new-window`; a file already open anywhere is brought forward instead of opened twice.

## Building and installing

    just build
    sudo just install        # or: just prefix=$HOME/.local install
    just set-default         # plain text opens in Chord

Build needs Rust 1.92, `blueprint-compiler`, `just` and the development packages for GTK,
libadwaita and GtkSourceView 5.

    just run [FILE…]         # debug build, with the schema compiled into target/schemas
    just check               # fmt, clippy -D warnings, blueprint, schema, validators
    just test                # the unit tests

GPL-3.0-or-later.
