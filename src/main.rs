//! The `safescope` executable. The command line itself arrives in M1.

use safescope::dataformatting::Msg;

fn main() {
    eprintln!(
        "{}",
        Msg::CliNotImplemented {
            version: env!("CARGO_PKG_VERSION").to_owned()
        }
    );
    std::process::exit(2);
}
