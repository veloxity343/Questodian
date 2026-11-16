//! The `questodian` command.
//!
//! ```text
//! questodian                      serve the app and open it in your browser (same as `serve`)
//! questodian roll 4d6dl1 adv+5    roll dice
//! questodian calc "mod(16) + prof(5)"
//! questodian srd fireball         look something up in the SRD
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use questodian_core::schemas::Edition;
use questodian_core::server::{self, App, Ui};
use questodian_core::{db, dice, srd};

#[derive(Parser)]
#[command(name = "questodian", version, about = "All-in-one 5e table manager.")]
struct Cli {
    /// Folder for the SQLite database (env: QUESTODIAN_DATA).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the web app (the default).
    Serve(ServeArgs),
    /// Roll dice, e.g. 2d20kh1+5, 8d6, 4d6dl1.
    Roll {
        #[arg(required = true)]
        expressions: Vec<String>,
        #[arg(short = 'n', long, default_value_t = 1)]
        times: u32,
        /// Show individual dice.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Calculator; supports dice, mod(score), prof(level).
    Calc {
        #[arg(required = true)]
        expression: Vec<String>,
    },
    /// Search the System Reference Document.
    Srd {
        query: Vec<String>,
        /// e.g. spells, monsters, rules, conditions.
        #[arg(short, long)]
        category: Option<String>,
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
        /// Print the best match in full.
        #[arg(short, long)]
        show: bool,
    },
}

#[derive(clap::Args, Default)]
struct ServeArgs {
    /// Use 0.0.0.0 to let players on your network connect.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8765)]
    port: u16,
    /// Don't open a browser window.
    #[arg(long)]
    no_browser: bool,
    /// The built UI to serve (default: the one next to the Python package, if built).
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command =
        cli.command.unwrap_or(Command::Serve(ServeArgs { host: "127.0.0.1".into(), port: 8765, ..Default::default() }));
    match command {
        Command::Serve(args) => serve(cli.data_dir, args),
        Command::Roll { expressions, times, verbose } => roll(&expressions, times, verbose),
        Command::Calc { expression } => calc(&expression.join(" ")),
        Command::Srd { query, category, limit, show } => {
            srd_lookup(&query.join(" "), category.as_deref().unwrap_or(""), limit, show)
        }
    }
}

fn serve(data_dir: Option<PathBuf>, args: ServeArgs) -> ExitCode {
    let data_dir = data_dir.unwrap_or_else(db::default_data_dir);
    let web_dir = args.web_dir.unwrap_or_else(|| PathBuf::from("src/questodian/web"));
    let ui = if web_dir.join("index.html").is_file() { Ui::Folder(web_dir) } else { Ui::Missing };

    let shown_host = if matches!(args.host.as_str(), "127.0.0.1" | "0.0.0.0" | "") { "localhost" } else { &args.host };
    let url = format!("http://{shown_host}:{}/", args.port);
    let runtime = tokio::runtime::Runtime::new().expect("start the async runtime");
    let result: anyhow::Result<()> = runtime.block_on(async {
        let database = db::Database::open(&data_dir).await?;
        if let Some(backup) = &database.backup {
            println!("Upgraded the database; the previous version is saved as {}", backup.display());
        }
        let database_path = database.path.clone();
        let app = App::new(database, ui);
        let address: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
        let listener = tokio::net::TcpListener::bind(address).await?;
        println!("Questodian {} running at {url}", env!("CARGO_PKG_VERSION"));
        println!("Database: {}", database_path.display());
        println!("Press Ctrl+C to stop.");
        if !args.no_browser {
            open_browser(&url);
        }
        axum::serve(listener, server::router(app))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        Ok(())
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open_browser(url: &str) {
    let opener = if cfg!(windows) {
        std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if opener.is_err() {
        println!("Open {url} in your browser.");
    }
}

fn roll(expressions: &[String], times: u32, verbose: bool) -> ExitCode {
    let mut ok = true;
    for expression in expressions {
        for _ in 0..times {
            match dice::roll(expression) {
                Ok(result) => {
                    let flag = if result.critical {
                        "  CRIT!"
                    } else if result.fumble {
                        "  fumble"
                    } else {
                        ""
                    };
                    let shown = if verbose { result.breakdown.replace("~~", "") } else { result.total.to_string() };
                    println!("{expression}: {shown}{flag}");
                }
                Err(e) => {
                    eprintln!("{expression}: error: {e}");
                    ok = false;
                    break;
                }
            }
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn calc(expression: &str) -> ExitCode {
    match dice::roll(expression) {
        Ok(result) => {
            println!("{}", result.total);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn srd_lookup(query: &str, category: &str, limit: usize, show: bool) -> ExitCode {
    let edition = Edition::E2014;
    let found = srd::search(edition, query, category, &[], limit, 0);
    let Some(best) = found.results.first() else {
        println!("No matches.");
        return ExitCode::FAILURE;
    };
    if found.results.len() == 1 || show {
        let entry = srd::get(edition, &best.id).expect("search results exist");
        println!("{}", entry.body.unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    for entry in &found.results {
        println!("{:<40} {}", entry.id, entry.summary);
    }
    if found.total > found.results.len() {
        println!("... {} more", found.total - found.results.len());
    }
    ExitCode::SUCCESS
}
