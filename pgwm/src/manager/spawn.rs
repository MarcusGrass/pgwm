use crate::error::Result;
use std::process::Stdio;
pub(crate) fn spawn(bin: &str, args: &[String]) -> Result<()> {
    std::process::Command::new(bin)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(args)
        .spawn()?;
    pgwm_core::debug!("Spawned {} with args {:?}", bin, args);
    Ok(())
}
