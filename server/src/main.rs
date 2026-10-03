//! lana backend server entry point: a thin CLI wrapper around the
//! [`lana_server`] library (all modes live in [`lana_server::cli`]).

fn main() {
    lana_server::cli::main();
}
