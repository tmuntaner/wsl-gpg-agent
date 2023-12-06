extern crate core;

mod gpg;
mod licenses;
mod ssh;

use crate::gpg::Gpg;
use crate::licenses::Licenses;
use crate::ssh::SshPageant;
use anyhow::{anyhow, Result};
use clap::Parser;
use flexi_logger::{FileSpec, Logger, WriteMode};
use std::io;

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
    Ssh(Ssh),
    Licenses(Licenses),
}

#[derive(Parser)]
pub struct Ssh {}

impl Ssh {
    pub fn run(&self) -> Result<()> {
        log::info!("start");

        let pageant_window_name = String::from("Pageant");
        let pageant_class_name = String::from("Pageant");

        loop {
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            let mut stdout = io::stdout();
            let pageant = SshPageant::new();
            pageant.run(
                &pageant_window_name,
                &pageant_class_name,
                &mut stdout,
                &mut reader,
            )?
        }
    }
}

fn main() -> Result<()> {
    let path = dirs::cache_dir()
        .ok_or_else(|| anyhow!("could not determine config directory"))?
        .join("wsl-gpg-agent");
    let _logger = Logger::try_with_str("info")?
        .log_to_file(FileSpec::default().suppress_timestamp().directory(path))
        .write_mode(WriteMode::BufferAndFlush)
        .start();
    let opt: Opts = Opts::parse();

    match opt.sub_command {
        SubCommand::Gpg(val) => val.run()?,
        SubCommand::Ssh(val) => val.run()?,
        SubCommand::Licenses(val) => val.run()?,
    }

    Ok(())
}
