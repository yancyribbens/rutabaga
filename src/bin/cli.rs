use std::path::PathBuf;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Commands {
    /// Wallet commands.
    #[command(subcommand)]
    Wallet(WalletCmd),
}

#[derive(Debug, Clone, clap::Subcommand)]
enum WalletCmd {
    /// TODO
    GenerateKeys {
        /// TODO
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// TODO
    PrintKeysFromKeysFile { path: PathBuf },
}

fn main() {
}
