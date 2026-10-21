//! The `saveprism` command line tool; all logic lives in the library.

fn main() {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let code = saveprism_cli::run(std::env::args_os(), &mut stdout, &mut stderr);
    std::process::exit(code);
}
