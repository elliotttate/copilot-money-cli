use clap::CommandFactory;
use clap_complete::{Shell, generate};

use super::Cli;

pub(super) fn run_completions(shell: Shell) -> anyhow::Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
