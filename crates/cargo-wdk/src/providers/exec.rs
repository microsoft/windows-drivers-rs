use crate::errors::CommandError;
use anyhow::Result;
use log::debug;
use mockall::automock;
use std::collections::HashMap;
use std::process::{Command, Output, Stdio};

#[derive(Debug)]
pub struct CommandExec {}

#[automock]
pub trait RunCommand {
    fn run<'a>(&self, command: &'a str,args: &'a [&'a str],env_vars: Option<&'a HashMap<&'a str, &'a str>>) -> Result<Output, CommandError>;
}

impl RunCommand for CommandExec {
     fn run<'a>(
        &self,
        command: &'a str,
        args: &'a [&'a str],
        env_vars: Option<&'a HashMap<&'a str, &'a str>>,
    ) -> Result<Output, CommandError> {
        debug!("Running: {} {:?}", command, args);
    
        let mut cmd = Command::new(command);
        cmd.args(args);
    
        if let Some(env) = env_vars {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }
    
        let output = cmd
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|child| child.wait_with_output())
            .map_err(CommandError::IoError)?;
    
        if !output.status.success() {
            return Err(CommandError::from_output(command, args, output));
        }
    
        debug!(
            "COMMAND: {}\n ARGS:{:?}\n OUTPUT: {}\n",
            command,
            args,
            String::from_utf8_lossy(&output.stdout)
        );
    
        Ok(output)
    }
    
}
