/*!
  A crate for parsing IRC bot commands via clap. Documentation is hopefully good,
  but not vetted by others, so any comments are very welcome.

  The core function of the library is provided through [new_irclap_future],
  which links together all necessary trait impls and config as a single [Future].
  That Future can then be driven on a tokio reactor, and you've got yourself an IRC
  bot.
  */

#![feature(conservative_impl_trait)]
extern crate irc;
extern crate clap;
extern crate futures;
extern crate tokio_core;

use irc::client::prelude as ircp;
use irc::client::prelude::{Client, ClientExt};
use irc::client::{PackedIrcClient};

use tokio_core::reactor::Handle;

use futures::{Future, Stream};

use std::borrow::Cow;

mod irc_util;

/**
 * Sends out messages to whatever channel, nickname, or other
 * target started processing of a message.
 *
 * Implementations will use `NOTICE` for channels,
 * `PRIVMSG` for direct messages, and just `println!`
 * for CLI execution.
 */
pub trait IrclapResponseStream {
    /**
     * Sends a single line to the appropriate response target.
     */
    fn send_message(&self, msg: &str) -> Result<(), irc::error::IrcError>;
}

/**
 * Provides a mapping between the context provided by an IRC message
 * and the actual arguments needed to run a clap application.
 * Also allows for special transformations to any arguments provided
 * in the message.
 */
pub trait IrclapContextMapper {
    /**
     * Performs mapping from an original semi-parsed message to a full command.
     *
     * `args` contains the arguments for the command, as
     * parsed from the message content. In general, this is just the message
     * split at whitespace, with the current nick of the bot stripped from the beginning.
     *
     * Must return the full list of arguments to be parsed by `clap::App`.
     */
    fn prepare_command_args<'a>(&'a self, args: Vec<&'a str>, msg: &'a ircp::Message) -> Vec<Cow<'a, str>>;
}

/**
 * Used to execute your program based on the results
 * of `irclap`'s message handling and parsing.
 *
 * An instance of this trait will be called into once parsing is
 * completed for a command. The
 * [process_matches][IrclapCommandProcessor::process_matches] function is
 * responsible for performing all actions necessary for handling the command.
 */
pub trait IrclapCommandProcessor {
    /**
     * Process the results of command parsing, and execute your application.
     *
     * The implementation of this function will provide essentially all the
     * actual 'business logic' of the program being run via `irclap`. Anything
     * you need to do that isn't setup, you do in this function.
     *
     * It's worth noting that there is no way to return an error code from this
     * function. This is partly an artifact of the IRC-first design. For IRC, the
     * only reasonable response to an error is to send a message about it. Do that.
     *
     * In the future, there may also be a way to provide an exit code for use in
     * CLI contexts, but that does not exist at the moment.
     */
    fn process_matches<'a, RS>(&self, matches: clap::ArgMatches<'a>, resp: RS)
        where RS: IrclapResponseStream + 'a;
}

impl<F> IrclapCommandProcessor for F where F: for<'af> Fn(clap::ArgMatches<'af>, Box<IrclapResponseStream + 'af>) {
    fn process_matches<'a, RS>(&self, matches: clap::ArgMatches<'a>, resp: RS)
        where RS: IrclapResponseStream + 'a{
        let rstream = Box::new(resp) as Box<IrclapResponseStream + 'a>;
        (self)(matches, rstream)
    }
}

/**
 * Supports extracting common context values from IRC messages
 * into args for processing.
 *
 * Commonly, an app may need to know the username and possibly
 * the channel a message was sent on. `irclap` handles this by
 * including them as arguments to the `clap::App` for parsing,
 * as this potentially allows reusing the `App` in CLI scenarios.
 */
pub struct IrclapSimpleContextMapping {
    pub channel: Option<String>,
    pub username: Option<String>,
}

impl IrclapSimpleContextMapping {
    /**
     * A context that doesn't pass any values along. Useful for
     * context-insensitive applications including factoids, unit conversion,
     * and basically anything stateless.
     */
    pub fn none() -> IrclapSimpleContextMapping {
        IrclapSimpleContextMapping{
            channel: None,
            username: None,
        }
    }

    /**
     * Maps the username of the message to an argument. Does not map the
     * channel at all.
     *
     * # Example:
     *
     * ```
     * # extern crate irc;
     * # extern crate irclap;
     * # use irc::client::prelude::Message;
     * # use irclap::{IrclapContextMapper, IrclapSimpleContextMapping};
     * //Context, along with a message "arg1 arg2" from 'someuser'
     * let context_mapping = IrclapSimpleContextMapping::user_only("--profile-name".to_owned());
     * let message = Message::new(Some("someuser"), "PRIVMSG", vec!["mybot"], Some("arg1 arg2")).unwrap();
     *
     * //Usually irclap extracts this from the message for us, but we'll hardcode it here
     * let message_args = vec!["arg1", "arg2"];
     *
     * // Now we proces the message and we get username passed as an argument.
     * let mapped = context_mapping.prepare_command_args(message_args, &message);
     * assert_eq!(vec!["arg1", "arg2", "--profile-name", "someuser"], mapped);
     * ```
     */
    pub fn user_only(username: String) -> IrclapSimpleContextMapping {
        IrclapSimpleContextMapping {
            channel: None,
            username: Some(username),
        }
    }
}

fn arg_tuple_opt<'a>(arg: &'a Option<String>, value: Option<&'a str>) -> Option<(&'a str, &'a str)> {
    arg.as_ref().map(String::as_str).and_then(|a| value.map(|v| (a, v)))
}

fn push_arg_tuple<'a>(args: &mut Vec<&'a str>, arg: &'a Option<String>, value: Option<&'a str>) {
    for (a, v) in arg_tuple_opt(arg, value) {
        args.push(a);
        args.push(v);
    }
}

impl IrclapContextMapper for IrclapSimpleContextMapping {
    fn prepare_command_args<'a>(&'a self, mut args: Vec<&'a str>, msg: &'a ircp::Message) -> Vec<Cow<'a, str>> {
        push_arg_tuple(&mut args, &self.channel, msg.response_target());
        push_arg_tuple(&mut args, &self.username, msg.source_nickname());
        args.into_iter().map(Cow::from).collect()
    }
}

struct IrclapProcessor<CM, CP> {
    mapper: CM,
    processor: CP,
}

impl<CM, CP> IrclapProcessor<CM, CP> {
    fn new(mapper: CM, processor: CP) -> IrclapProcessor<CM, CP> {
        IrclapProcessor {
            mapper: mapper,
            processor: processor,
        }
    }
}

fn process_single_message<'a, CM, CP>(
    app: clap::App<'a, 'a>,
    context: &IrclapProcessor<CM, CP>,
    client: &ircp::IrcClient,
    msg: ircp::Message)
    where CM: IrclapContextMapper,
          CP: IrclapCommandProcessor {
    if let Some(command) = irc_util::extract_command(client.current_nickname(), &msg) {
        let args:Vec<&str> = command.split_whitespace().collect();
        let args = context.mapper.prepare_command_args(args, &msg);

        /* We don't process messages without response targets,
         * so it's ok to unwrap here.
         * (see process_message_streams for filtering)
         */
        let out_stream = irc_util::IrcResponseStream::new(&client, msg.response_target().unwrap());

        match app.get_matches_from_safe(args.iter().map(Cow::as_ref)) {
            Ok(matches) => context.processor.process_matches(matches, out_stream),
            Err(e) => {
                //TODO: Logging of some sort?
                let _ = out_stream.send_message(&format!("Argument error: {:?}", e));
            }
        }
    }

}

fn process_message_streams<'a, CM, CP>(
    app: clap::App<'a, 'a>,
    context: IrclapProcessor<CM, CP>,
    client: ircp::IrcClient)
    -> impl Future<Item=(), Error=irc::error::IrcError> + 'a
    where CM: IrclapContextMapper + 'a,
          CP: IrclapCommandProcessor + 'a {
    client
        .stream()
        .filter(|m| {println!("{:?}", m); m.response_target().is_some()})
        .for_each(move |msg| {
            process_single_message(app.clone(), &context, &client, msg);
            Ok(())
        })
}

/**
 * Create a new [Future] which will execute an `irclap` application.
 *
 * You will need the tokio reactor [Core][tokio_core::reactor::Core] to drive the resulting future. You
 * MUST have direct access to the Core. A [Handle], while sufficient to create the future,
 * is insufficient to run it. Handle requires a Future bounded with `'static`, which
 * this will almost certainly not provide. Future changes will probably make this more
 * embedded in the function signature.
 *
 * For configuration, note that the irc config must have info needed to
 * identify with the IRC server, along with all the necessary options for
 * connecting in the first place. The future will identify with the server
 * using the nickname and identification setup from your config.
 *
 * The [App][clap::App] has essentially no restrictions, but you should note that
 * the [NoBinaryName][clap::AppSettings::NoBinaryName] app setting will be added to the application
 * for commands over IRC.
 *
 * The mapper handles preprocessing the command to pass it to your
 * clap App for matching, and can be highly customized, but in most cases using
 * an [IrclapSimpleContextMapping] should probably be sufficient.
 *
 * The processor is the core part of your application, and is where all the
 * business logic or anything like that should happen.
 */
pub fn new_irclap_future<'a, CM, CP>(
    handle: Handle,
    cfg: &'a ircp::Config,
    app: clap::App<'a, 'a>,
    mapper: CM,
    processor: CP)
    -> impl Future<Item=(), Error=irc::error::IrcError> + 'a
    where CM: IrclapContextMapper + 'a,
          CP: IrclapCommandProcessor + 'a{
    let ctxt = IrclapProcessor::new(mapper, processor);

    //At least as of irc 0.13.4, this never fails
    let irc_client_creator = ircp::IrcClient::new_future(handle, cfg).unwrap();

    let complete_app = app.setting(clap::AppSettings::NoBinaryName);

    irc_client_creator
        //item 0 is the actual irc client
        .and_then(|packed_client| packed_client.0.identify().map(|_| packed_client))
        .and_then(move |PackedIrcClient(client, future)| {
            //drive both sends (future) and processing (the process_message_streams result)
            future.join(process_message_streams(complete_app, ctxt, client))
        }).map(|_| ())
}
