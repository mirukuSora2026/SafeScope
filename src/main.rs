//! The `safescope` executable.

use clap::Parser as _;

use safescope::cli::{Cli, run};

fn main() {
    std::process::exit(run(Cli::parse()));
}
