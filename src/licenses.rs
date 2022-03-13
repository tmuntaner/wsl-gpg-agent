use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
pub struct Licenses {}

impl Licenses {
    pub fn run(&self) -> Result<()> {
        let my_str = include_str!("../license.txt");
        print!("{}", my_str);

        Ok(())
    }
}
