use git_trim::args::Args;
use man::prelude::*;
use structopt::StructOpt;

fn main() {
    // let dummy_default_args = Args {
    //     bases: Default::default(),
    //     protected: Default::default(),
    //     no_update: Default::default(),
    //     update: Default::default(),
    //     no_confirm: Default::default(),
    //     confirm: Default::default(),
    //     no_detach: Default::default(),
    //     detach: Default::default(),
    //     delete: Default::default(),
    //     dry_run: Default::default(),
    // };

    let clap_app = Args::clap();
    // clap_app.

    // let package_name = env!("CARGO_PKG_NAME");
    // let authors = env!("CARGO_PKG_AUTHORS");

    // Args::
    let manual = Manual::new(clap_app.get_name());
    let parser = clap_app.p;
    let has_flags = parser.has_flags();

    if has_flags {
        let flags = parser.flags();
        for flag in flags {
            println!("Flag {:?}", flag);
        }
    }
    // let args = clap_app.p.global_args;
    // for arg in &args {
    //     manual.flag(Flag::new().)
    //     arg.b.name
    // }
    let page = manual.render();

    println!("{}", page);
}
