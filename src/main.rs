use clap::Parser;

use svg2tex_rs::{run, Args};

fn main() {
    let args = Args::parse();

    if let Err(err) = run(args) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
