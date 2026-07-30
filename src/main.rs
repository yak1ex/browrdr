#![windows_subsystem = "windows"]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, CommandFactory, Parser};
use home;
use regex::Regex;
use serde::Deserialize;
use toml;
use win_msgbox;
use windows_registry::{CURRENT_USER, Key, Transaction};

#[derive(Deserialize, Debug)]
struct Browser {
    regex: Option<String>,
    command: String,
    args: Vec<String>
}

#[derive(Deserialize, Debug)]
struct Config {
    browser: Vec<Browser>
}

#[derive(Parser)]
#[command(version)]
#[command(about = "A tiny browser redirector based on URL match")]
#[command(disable_help_flag = true)]
#[command(disable_help_subcommand = true)]
#[command(disable_version_flag = true)]
struct Cli {
    /// Mode
    #[command(flatten)]
    mode: Mode,

    /// Optional config TOML path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// URL to be given to actual browser, ignored if a mode is provided
    #[arg(required_unless_present_any = ["install", "uninstall", "help", "version"])]
    url: Option<String>,
}

#[derive(Args)]
#[group(required = false, multiple = false)]
struct Mode {
    /// Specify a mode to add registry entries, not yet implemented
    #[arg(short, long)]
    install: bool,

    /// Specify a mode to remove registry entries, not yet implemented
    #[arg(short, long)]
    uninstall: bool,

    /// Specify a mode to show help messages
    #[arg(short, long)]
    help: bool,

    /// Specify a mode to show version info
    #[arg(short, long)]
    version: bool,
}

fn message(message_content: &str) -> Result<()> {
    win_msgbox::information::<win_msgbox::Okay>(message_content)
        .title("browrdr").show().or_else(|value| Err(anyhow!("win32 error {}", value)))?;
    Ok(())
}

enum CliMessage {
  Help,
  Version
}

fn cli_message(mes: CliMessage) -> Result<()> {
    let message_content = match mes {
        CliMessage::Help => Cli::command().render_help().to_string(),
        CliMessage::Version => Cli::command().render_version().to_string()
    };
    message(&message_content)
}

/*
[HKEY_CURRENT_USER\SOFTWARE\Clients\StartMenuInternet\Browrdr]
@="Browrdr"

[HKEY_CURRENT_USER\SOFTWARE\Clients\StartMenuInternet\Browrdr\Capabilities]
"ApplicationDescription"="Browrdr"
"ApplicationName"="Browrdr"

[HKEY_CURRENT_USER\SOFTWARE\Clients\StartMenuInternet\Browrdr\Capabilities\Startmenu]
"StartmenuInternet"="Browrdr"

[HKEY_CURRENT_USER\SOFTWARE\Clients\StartMenuInternet\Browrdr\Capabilities\URLAssociations]
"http"="Browrdr"
"https"="Browrdr"

[HKEY_CURRENT_USER\SOFTWARE\Classes\Browrdr\Application]
"ApplicationDescription"="Browrdr"
"ApplicationName"="Browrdr"

[HKEY_CURRENT_USER\SOFTWARE\Classes\Browrdr\shell\open\command]
@="\"<self_path>\" \"%1\""

[HKEY_CURRENT_USER\SOFTWARE\RegisteredApplications]
"Browrdr"="Software\\Clients\\StartMenuInternet\\Browrdr\\Capabilities"
*/

const APP_NAME : &str = "Browrdr";

fn open_key_for_write(root: &Key, tx: &Transaction, path: &str) -> Result<Key> {
    let ret = root.options().read().write().transaction(tx).create().open(path)?;
    Ok(ret)
}

fn install() -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let tx = Transaction::new()?;
    let key_app = open_key_for_write(CURRENT_USER, &tx, &format!("Software\\Classes\\{}\\Application", APP_NAME))?;
    key_app.set_string("ApplicationDescription", APP_NAME)?;
    key_app.set_string("ApplicationName", APP_NAME)?;
    let key_command = open_key_for_write(CURRENT_USER, &tx, &format!("Software\\Classes\\{}\\shell\\open\\command", APP_NAME))?;
    key_command.set_string("", format!("\"{}\" \"%1\"", current_exe.display()))?;
    let key_menu = open_key_for_write(CURRENT_USER, &tx, &format!("Software\\Clients\\StartMenuInternet\\{}", APP_NAME))?;
    key_menu.set_string("", APP_NAME)?;
    let key_cap = open_key_for_write(&key_menu, &tx, "Capabilities")?;
    key_cap.set_string("ApplicationDescription", APP_NAME)?;
    key_cap.set_string("ApplicationName", APP_NAME)?;
    let key_start_menu = open_key_for_write(&key_cap, &tx, "Startmenu")?;
    key_start_menu.set_string("StartmenuInternet", APP_NAME)?;
    let key_url_assoc = open_key_for_write(&key_cap, &tx, "URLAssociations")?;
    key_url_assoc.set_string("http", APP_NAME)?;
    key_url_assoc.set_string("https", APP_NAME)?;
    let key_reg_app = open_key_for_write(CURRENT_USER, &tx, "Software\\RegisteredApplications")?;
    key_reg_app.set_string(APP_NAME, format!("Software\\Clients\\StartMenuInternet\\{}\\Capabilities", APP_NAME))?;
    tx.commit()?;
    message("Registry entries for browser selection are added.")?;
    Ok(())
}

fn open_key_for_remove(root: &Key, tx: &Transaction, path: &str) -> Result<Key> {
    let ret = root.options().write().transaction(tx).open(path)?;
    Ok(ret)
}

fn uninstall() -> Result<()> {
    let tx = Transaction::new()?;
    let key_software = open_key_for_remove(CURRENT_USER, &tx, "Software")?;
    key_software.remove_tree(&format!("Classes\\{}", APP_NAME))?;
    key_software.remove_tree(&format!("Clients\\StartMenuInternet\\{}", APP_NAME))?;
    let key_reg_app = open_key_for_remove(&key_software, &tx, "RegisteredApplications")?;
    key_reg_app.remove_value(APP_NAME)?;
    tx.commit()?;
    message("Registry entries for browser selection are removed.")?;
    Ok(())
}

fn default_path() -> Result<PathBuf> {
    return Ok(home::home_dir().ok_or(anyhow!("Can't detect home dir"))?.join(".config/browrdr/config.toml"))
}

fn process(cli : Cli) -> Result<()> {
    let config_path = cli.config.map_or_else(default_path, Ok)?;
    let config_content = fs::read_to_string(&config_path)
        .with_context(|| format!("Can't read the config file: {}", config_path.display()))?;
    let config: Config = toml::from_str(&config_content)
        .with_context(|| format!("Can't parse the config file: {} as TOML", config_path.display()))?;
    if let Some(url) = cli.url {
        for browser in config.browser {
            let ok = if let Some(regex) = browser.regex {
                let re = Regex::new(&regex)?;
                re.is_match(&url)
            } else {
                true
            };
            if ok {
                let _ = Command::new(browser.command)
                .args(browser.args)
                .arg(url)
                .spawn()?;
                return Ok(());
            }
        }
        bail!("target browser not found for URL {}", url);
    }
    bail!("no URL specified")
}

fn actual_main() -> Result<()> {
    let cli = Cli::try_parse()?;

    if cli.mode.install {
        install()
    } else if cli.mode.uninstall {
        uninstall()
    } else if cli.mode.help {
        cli_message(CliMessage::Help)
    } else if cli.mode.version {
        cli_message(CliMessage::Version)
    } else {
        process(cli)
    }
}

fn main() -> ExitCode {
    let ret = if let Err(err) = actual_main() {
        let error_message = format!("{:#}", err);
        let _ = win_msgbox::error::<win_msgbox::Okay>(&error_message)
            .title("browrdr").show();
        1
    } else {
        0
    };
    ret.into()
}
