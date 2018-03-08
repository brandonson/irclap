use irc::client::prelude as ircp;
use irc::client::prelude::ChannelExt;
use irc::client::prelude::ClientExt;

use IrclapResponseStream;

pub(crate) struct IrcResponseStream<'c> {
    client: &'c ircp::IrcClient,
    response_target: &'c str,
}

impl<'c> IrcResponseStream<'c> {
    pub(crate) fn new(client: &'c ircp::IrcClient, rt: &'c str) -> IrcResponseStream<'c> {
        IrcResponseStream {
            client: client,
            response_target: rt,
        }
    }
}

impl<'c> IrclapResponseStream for IrcResponseStream<'c> {
    fn send_message(&self, msg: &str) -> Result<(), ::irc::error::IrcError>{
        if self.response_target.is_channel_name() {
            self.client.send_notice(self.response_target, msg)
        } else {
            self.client.send_privmsg(self.response_target, msg)
        }
    }
}

fn strip_botname<'m>(botname: &str, msg: &'m str) -> Option<&'m str> {
    if msg.starts_with(botname) {
        Some(&msg[botname.len()..].trim_matches(':').trim_matches(',').trim())
    } else {
        None
    }
}

pub(crate) fn extract_command<'m>(botname:&str, msg: &'m ircp::Message) -> Option<&'m str> {
    let is_channel = msg.response_target().map(|rt| rt.is_channel_name()).unwrap_or(false);
    match msg.command {
        ircp::Command::PRIVMSG(_, ref m) if is_channel => strip_botname(botname, m),
        ircp::Command::PRIVMSG(_, ref m) => strip_botname(botname, m).or(Some(m)),
        _ => None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use irc::client::prelude::Message;

    #[test]
    fn can_strip_botname_from_msg() {
        let msg = Message::new(Some("usr"), "PRIVMSG", vec!["#chan"], Some("bot: Hi")).unwrap();
        assert_eq!(extract_command("bot", &msg), Some("Hi"));

        let msg = Message::new(Some("usr"), "PRIVMSG", vec!["usr"], Some("bot: Hi")).unwrap();
        assert_eq!(extract_command("bot", &msg), Some("Hi"));
    }

}
