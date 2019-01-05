#[macro_use]
extern crate structopt;
extern crate cargo_prepopulate;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "cargo-prepopulate")]
struct Opts {
    /// Activate debug mode
    //#[structopt(short = "d", long = "debug")]
    //debug: bool,

    /// Path to Cargo-project
    #[structopt(name = "PATH", parse(from_os_str))]
    path: PathBuf,
}

fn main() {
    let opt = Opts::from_args();
    std::process::exit(match cargo_prepopulate::prepopulate(&opt.path) {
        Err(err) => {
            println!("{:?}", err);
            1
        },
    Ok(_) => 0
    });
}
