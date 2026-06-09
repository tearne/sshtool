//! Running a command on an authenticated session, streaming its output to the
//! local terminal and reporting the remote exit status.

use anyhow::Result;
use russh::client::{Handle, Msg};
use russh::{Channel, ChannelMsg};
use tokio::io::{AsyncWriteExt, stderr, stdout};

use crate::connection::Client;

pub(crate) async fn run(session: &Handle<Client>, command: &str) -> Result<u32> {
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, command).await?;
    stream_until_exit(&mut channel).await
}

/// Pump the channel to the local terminal until it closes — stdout to stdout,
/// stderr to stderr, as the bytes arrive. Returns the remote command's exit
/// status, or 0 if the remote closed without reporting one.
async fn stream_until_exit(channel: &mut Channel<Msg>) -> Result<u32> {
    let mut local_stdout = stdout();
    let mut local_stderr = stderr();
    let mut exit_status = 0;

    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { ref data } => {
                local_stdout.write_all(data).await?;
                local_stdout.flush().await?;
            }
            ChannelMsg::ExtendedData { ref data, .. } => {
                local_stderr.write_all(data).await?;
                local_stderr.flush().await?;
            }
            ChannelMsg::ExitStatus { exit_status: status } => {
                exit_status = status;
            }
            _ => {}
        }
    }

    Ok(exit_status)
}
