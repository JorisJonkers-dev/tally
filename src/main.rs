// SPDX-License-Identifier: GPL-3.0-only
//! The `tally` program.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = tally::cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
