use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
#[command(name = "Browser selector")]
#[command(version = "0.1.0")]
#[command(author = "Yak! <yak_ex@mx.scn.tv>")]
#[command(about = "Brwowser selector based on URL match")]
struct Cli {
    /// Optional config TOML path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// URL to be given to actual browser
    url: String
}
fn main() {
    let cli = Cli::parse();

    if let Some(config_path) = cli.config.as_deref() {
        println!("Value for config: {}", config_path.display())
    }
    println!("Value for URL: {}", cli.url)
}
