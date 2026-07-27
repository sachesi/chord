# Chord

Chord is a text editor written in Rust with GTK 4, libadwaita and GtkSourceView 5.

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
