mod gpg;
mod licenses;

use crate::gpg::Gpg;
use crate::licenses::Licenses;
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[clap(
    version = "1.0",
    author = "Thomas Muntaner <thomas.muntaner@gmail.com>"
)]
struct Opts {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    Gpg(Gpg),
    Licenses(Licenses),
}

fn main() -> Result<()> {
    let opt: Opts = Opts::parse();

    match opt.sub_command {
        SubCommand::Gpg(val) => val.run()?,
        SubCommand::Licenses(val) => val.run()?,
    }

    Ok(())
}
