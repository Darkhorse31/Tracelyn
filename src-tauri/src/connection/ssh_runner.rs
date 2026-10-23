use crate::collectors::redis::RemoteCommandRunner;
use crate::ssh::{run_command, SshSession};
use crate::webserver::RemoteExec;
use std::future::Future;
use std::pin::Pin;

pub struct SessionRunner<'a> {
    session: &'a SshSession,
}

impl<'a> SessionRunner<'a> {
    pub fn new(session: &'a SshSession) -> Self {
        Self { session }
    }
}

impl RemoteExec for SessionRunner<'_> {
    type Error = std::io::Error;

    fn run_command<'b>(
        &'b self,
        command: &'b str,
    ) -> Pin<Box<dyn Future<Output = Result<String, Self::Error>> + Send + 'b>> {
        Box::pin(async move {
            run_command(self.session, command)
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        })
    }
}

impl<'a> RemoteCommandRunner for SessionRunner<'a> {
    fn run<'b>(
        &'b self,
        command: &'b str,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'b>> {
        Box::pin(async move {
            run_command(self.session, command)
                .await
                .map_err(|e| e.to_string())
        })
    }
}
