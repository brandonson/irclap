# irclap

A bridge between [the irc crate](https://www.github.com/aatxe/irc) and [clap-rs](https://www.github.com/kbknapp/clap-rs).
This helps process irc messages as commands, and will also hopefully allow
accessing bot commands as command line tools (and vice versa)

Of course, being version `0.1.0`, there's a lot of room for improvement.

[Docs on docs.rs](https://docs.rs/irclap)

## Limitations

* Currently nightly-only, because it needs `conservative_impl_trait`.
* No CLI support yet
