#[cfg(windows)]
mod win;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        return win::main();
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("zagens-command-runner is only supported on Windows");
    }
}
