/// wt-ssh-manager — standalone Rust binary (no Python required).
///
/// Passwords are encrypted with Windows DPAPI (bound to current OS user).
/// Server profiles are injected into Windows Terminal via JSON Fragment Extensions.
mod config;
mod crypto;
mod fragment;
mod ssh_session;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, Table};
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, Password};

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "ssh-manager",
    version,
    about = "\u{1f5a5}  Windows Terminal SSH Manager\n\nStandalone binary \u{2014} no Python required.\nPasswords encrypted with Windows DPAPI (current OS user only)."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new SSH server (interactive wizard)
    Add,
    /// List all configured SSH servers
    List,
    /// Remove a server
    Remove {
        /// Server name or id
        name: String,
    },
    /// Edit a server's configuration
    Edit {
        /// Server name or id
        name: String,
    },
    /// Regenerate Windows Terminal Fragment profiles
    Sync,
    /// Test the SSH connection to a server
    Test {
        /// Server name or id
        name: String,
    },
    /// Connect to a server (interactive picker when NAME is omitted)
    Connect {
        /// Server name or id (optional)
        name: Option<String>,
    },
}

// ── entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{} {}", "❌".red(), e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Add              => cmd_add()?,
        Commands::List             => cmd_list()?,
        Commands::Remove { name }  => cmd_remove(&name)?,
        Commands::Edit   { name }  => cmd_edit(&name)?,
        Commands::Sync             => cmd_sync()?,
        Commands::Test   { name }  => cmd_test(&name).await?,
        Commands::Connect { name } => cmd_connect(name.as_deref()).await?,
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

fn auto_sync(cfg: &config::ConfigManager) -> Result<()> {
    let (_, count) = fragment::sync_fragment(cfg)?;
    println!(
        "  {} Fragment updated ({} profile(s)) \u{2014} restart Windows Terminal to apply.",
        "↺".dimmed(),
        count
    );
    Ok(())
}

// ── add ───────────────────────────────────────────────────────────────────────

fn cmd_add() -> Result<()> {
    let t = theme();
    println!("\n  {}", "Add New SSH Server".cyan().bold());
    println!("  {}", "─".repeat(32));

    let name: String =
        Input::with_theme(&t).with_prompt("  Server name (e.g. prod-web)").interact_text()?;
    let host: String =
        Input::with_theme(&t).with_prompt("  Host / IP address").interact_text()?;
    let port: u16 =
        Input::with_theme(&t).with_prompt("  Port").default(22u16).interact_text()?;
    let username: String =
        Input::with_theme(&t).with_prompt("  Username").interact_text()?;
    let password =
        Password::with_theme(&t).with_prompt("  Password").interact()?;
    let description: String =
        Input::with_theme(&t).with_prompt("  Description (optional)").allow_empty(true).interact_text()?;

    let mut mgr = config::ConfigManager::load()?;
    let srv = mgr.add_server(&name, &host, port, &username, &password, &description)?;

    println!(
        "\n  {} Server \"{}\" ({}@{}:{}) added.",
        "✅".green(),
        srv.name.bold(),
        srv.username,
        srv.host,
        srv.port
    );
    auto_sync(&mgr)
}

// ── list ──────────────────────────────────────────────────────────────────────

fn cmd_list() -> Result<()> {
    let mgr = config::ConfigManager::load()?;
    let servers = mgr.list_servers();

    if servers.is_empty() {
        println!("No servers configured. Run {} to add one.", "ssh-manager add".cyan());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("Name").add_attribute(Attribute::Bold).fg(Color::Magenta),
        Cell::new("Host").add_attribute(Attribute::Bold).fg(Color::Magenta),
        Cell::new("Port").add_attribute(Attribute::Bold).fg(Color::Magenta),
        Cell::new("Username").add_attribute(Attribute::Bold).fg(Color::Magenta),
        Cell::new("Description").add_attribute(Attribute::Bold).fg(Color::Magenta),
    ]);
    for s in servers {
        table.add_row(vec![
            Cell::new(&s.name).add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(&s.host),
            Cell::new(s.port.to_string()),
            Cell::new(&s.username).fg(Color::Green),
            Cell::new(if s.description.is_empty() { "—" } else { &s.description }),
        ]);
    }
    println!("{table}");
    Ok(())
}

// ── remove ────────────────────────────────────────────────────────────────────

fn cmd_remove(name: &str) -> Result<()> {
    let mut mgr = config::ConfigManager::load()?;
    let srv = mgr
        .get_server(name)
        .ok_or_else(|| anyhow::anyhow!("Server \"{}\" not found", name))?
        .clone();

    let ok = Confirm::with_theme(&theme())
        .with_prompt(format!(
            "  Remove {} ({}@{})?",
            srv.name.bold(),
            srv.username,
            srv.host
        ))
        .interact()?;

    if ok {
        mgr.remove_server(name)?;
        println!("  {} Server \"{}\" removed.", "✅".green(), srv.name.bold());
        auto_sync(&mgr)?;
    } else {
        println!("  Cancelled.");
    }
    Ok(())
}

// ── edit ──────────────────────────────────────────────────────────────────────

fn cmd_edit(name: &str) -> Result<()> {
    let mut mgr = config::ConfigManager::load()?;
    let srv = mgr
        .get_server(name)
        .ok_or_else(|| anyhow::anyhow!("Server \"{}\" not found", name))?
        .clone();

    let t = theme();
    println!("\n  {} {}  (Enter = keep current)", "Edit:".cyan().bold(), srv.name.bold());
    println!("  {}", "─".repeat(40));

    let new_host: String =
        Input::with_theme(&t).with_prompt("  Host").default(srv.host.clone()).interact_text()?;
    let new_port: u16 =
        Input::with_theme(&t).with_prompt("  Port").default(srv.port).interact_text()?;
    let new_user: String =
        Input::with_theme(&t).with_prompt("  Username").default(srv.username.clone()).interact_text()?;

    let change_pw =
        Confirm::with_theme(&t).with_prompt("  Change password?").default(false).interact()?;
    let new_pw = if change_pw {
        Some(Password::with_theme(&t).with_prompt("  New password").interact()?)
    } else {
        None
    };

    let new_desc: String = Input::with_theme(&t)
        .with_prompt("  Description")
        .default(srv.description.clone())
        .allow_empty(true)
        .interact_text()?;

    mgr.update_server(
        name,
        Some(&new_host),
        Some(new_port),
        Some(&new_user),
        new_pw.as_deref(),
        Some(&new_desc),
    )?;

    println!("  {} Server \"{}\" updated.", "✅".green(), srv.name.bold());
    auto_sync(&mgr)
}

// ── sync ──────────────────────────────────────────────────────────────────────

fn cmd_sync() -> Result<()> {
    let mgr = config::ConfigManager::load()?;
    let (path, count) = fragment::sync_fragment(&mgr)?;
    println!("  {} Fragment written to:", "✅".green());
    println!("     {}", path.display().to_string().dimmed());
    println!("     {} profile(s) injected.", count);
    println!("     Restart Windows Terminal to apply changes.");
    Ok(())
}

// ── test ──────────────────────────────────────────────────────────────────────

async fn cmd_test(name: &str) -> Result<()> {
    let mgr = config::ConfigManager::load()?;
    let srv = mgr
        .get_server(name)
        .ok_or_else(|| anyhow::anyhow!("Server \"{}\" not found", name))?
        .clone();
    let pw = mgr.get_password(&srv)?;

    print!("  Testing {}@{}:{}  ...", srv.username, srv.host, srv.port);
    std::io::Write::flush(&mut std::io::stdout())?;

    match ssh_session::test_connection(&srv.host, srv.port, &srv.username, &pw).await {
        Ok((banner, ms)) => {
            println!(
                "\r  {} {} reachable ({} ms)       ",
                "✅".green(),
                srv.name.bold(),
                ms
            );
            if !banner.is_empty() {
                println!("     {}", banner.dimmed());
            }
        }
        Err(e) => {
            println!("\r  {} Connection failed: {}       ", "❌".red(), e);
        }
    }
    Ok(())
}

// ── connect ───────────────────────────────────────────────────────────────────

async fn cmd_connect(name: Option<&str>) -> Result<()> {
    let mgr = config::ConfigManager::load()?;

    let server_id: String = match name {
        Some(n) => n.to_string(),
        None => {
            let servers = mgr.list_servers();
            if servers.is_empty() {
                println!(
                    "No servers configured. Run {} to add one.",
                    "ssh-manager add".cyan()
                );
                return Ok(());
            }
            let items: Vec<String> = servers
                .iter()
                .map(|s| format!("{:<22} {}@{}:{}", s.name, s.username, s.host, s.port))
                .collect();
            let sel = FuzzySelect::with_theme(&theme())
                .with_prompt("Select server")
                .items(&items)
                .default(0)
                .interact()?;
            servers[sel].id.clone()
        }
    };

    let srv = mgr
        .get_server(&server_id)
        .ok_or_else(|| anyhow::anyhow!("Server \"{}\" not found", server_id))?
        .clone();
    let pw = mgr.get_password(&srv)?;

    println!(
        "\n  \u{1f50c}  Connecting to {}@{}:{} ...",
        srv.username, srv.host, srv.port
    );
    ssh_session::interactive_connect(&srv.host, srv.port, &srv.username, &pw).await
}
