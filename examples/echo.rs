/*
 * IMPORTANT!!!
 * Needs irc config file echo-config.toml added to the examples directory
 * to work.
 */


extern crate irclap;

#[macro_use]
extern crate clap;

extern crate irc;
extern crate tokio_core;

use irc::client::prelude::*;
use tokio_core::reactor::Core;

fn echo_matches<'a>(matches: clap::ArgMatches<'a>, responder: Box<irclap::IrclapResponseStream + 'a>) {
    let echo:Vec<&str> = matches.values_of("ECHO").map(|v| v.collect()).unwrap_or(vec![]);
    let message = if echo.len() > 0 {
        echo.join(" ")
    } else {
        "A vast silence reigns because you didn't send anything to echo".to_owned()
    };
    responder.send_message(&message);
}

fn main() {
    let clap_yaml = load_yaml!("echo-args.yml");
    let app = clap::App::from_yaml(clap_yaml);

    let irc_conf = Config::load("examples/echo-config.toml").unwrap();

    let mut core = Core::new().unwrap();

    let cm = irclap::IrclapSimpleContextMapping::none();
    let irclap = irclap::new_irclap_future(core.handle(), &irc_conf, app, cm, echo_matches);

    println!("{:?}", core.run(irclap))
}
