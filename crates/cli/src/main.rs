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
use questodian_core::kinds::Kind;
use questodian_core::schemas::Edition;
use questodian_core::server::{self, App, Ui};
use questodian_core::{db, dice, srd, transfer};
use serde_json::Value;

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
    /// Export data to a JSON bundle.
    Export {
        /// File to write (default: print it).
        #[arg(default_value = "-")]
        output: String,
        /// Export only this campaign and its linked records.
        #[arg(long)]
        campaign: Option<String>,
    },
    /// Import JSON exports or Markdown handbook files.
    Import {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long, default_value = "overwrite", value_parser = ["overwrite", "skip", "copy"])]
        mode: String,
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
    /// The built UI to serve (default: frontend/dist, if built).
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
        Command::Export { output, campaign } => run_with_db(cli.data_dir, |db| export(db, output, campaign)),
        Command::Import { files, mode } => run_with_db(cli.data_dir, |db| import(db, files, mode)),
        Command::Srd { query, category, limit, show } => {
            srd_lookup(&query.join(" "), category.as_deref().unwrap_or(""), limit, show)
        }
    }
}

fn serve(data_dir: Option<PathBuf>, args: ServeArgs) -> ExitCode {
    let data_dir = data_dir.unwrap_or_else(db::default_data_dir);
    let web_dir = args.web_dir.unwrap_or_else(|| PathBuf::from("frontend/dist"));
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

/// Open the database and run an async command against it.
fn run_with_db<F, Fut>(data_dir: Option<PathBuf>, command: F) -> ExitCode
where
    F: FnOnce(db::Database) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<ExitCode>>,
{
    let runtime = tokio::runtime::Runtime::new().expect("start the async runtime");
    let result = runtime.block_on(async {
        let database = db::Database::open(data_dir.unwrap_or_else(db::default_data_dir)).await?;
        command(database).await
    });
    result.unwrap_or_else(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

async fn export(database: db::Database, output: String, campaign: Option<String>) -> anyhow::Result<ExitCode> {
    let mut repo = database.read().await?;
    let bundle = transfer::export_bundle(&mut repo, campaign.as_deref()).await?;
    let text = serde_json::to_string_pretty(&bundle)?;
    if output == "-" {
        println!("{text}");
    } else {
        std::fs::write(&output, text)?;
        let counts: Vec<String> = bundle["collections"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(kind, docs)| format!("{} {kind}", docs.as_array().map_or(0, Vec::len)))
            .collect();
        println!("Exported {} to {output}", counts.join(", "));
    }
    // On stderr, so it's seen even when the export itself goes to stdout.
    // The file is written either way; 2 tells a backup script it's incomplete (1 is a failed export).
    let incomplete = warn_unreadable(&bundle, "can't be read and", "in the export");
    Ok(if incomplete { ExitCode::from(2) } else { ExitCode::SUCCESS })
}

/// Records an export left out (`unreadable` in a bundle or an import summary), as a warning.
/// Whether there were any.
fn warn_unreadable(report: &Value, why: &str, place: &str) -> bool {
    let Some(Value::Array(unreadable)) = report.get("unreadable") else { return false };
    let records = match unreadable.len() {
        0 => return false,
        1 => format!("1 record {why} isn't"),
        n => format!("{n} records {why} aren't"),
    };
    eprintln!("warning: {records} {place}:");
    for record in unreadable {
        let field = |key| record[key].as_str().unwrap_or("");
        let id = if field("id").is_empty() { "(no id)" } else { field("id") };
        eprintln!("  {} {id}: {}", field("kind"), field("error"));
    }
    true
}

async fn import(database: db::Database, files: Vec<PathBuf>, mode: String) -> anyhow::Result<ExitCode> {
    let mode = transfer::Mode::parse(&mode)?;
    let mut status = ExitCode::SUCCESS;
    for path in files {
        let name = path.display();
        let text = std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("{name}: {e}"))?;
        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ["md", "markdown", "txt"].contains(&extension.as_str()) {
            let filename = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            let mut repo = database.write().await?;
            let entry = repo.create(Kind::Handbook, Some(transfer::markdown_to_handbook(&text, &filename))).await?;
            repo.commit().await?;
            println!("{name}: added handbook entry '{}'", entry["title"].as_str().unwrap_or(""));
            continue;
        }
        let outcome = async {
            let payload: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("not valid JSON: {e}"))?;
            let mut repo = database.write().await?;
            let summary = transfer::import_data(&mut repo, &payload, mode).await?;
            repo.commit().await?;
            anyhow::Ok(summary)
        }
        .await;
        match outcome {
            Ok(summary) => {
                let list = |key: &str| {
                    summary[key]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .map(|(kind, n)| format!("{n} {kind}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let done = Some(list("imported")).filter(|s| !s.is_empty()).unwrap_or_else(|| "nothing".into());
                let skipped = list("skipped");
                let skipped = if skipped.is_empty() { String::new() } else { format!(" (skipped {skipped})") };
                println!("{name}: imported {done}{skipped}");
                warn_unreadable(&summary, "couldn't be read when this file was made and", "in it");
            }
            Err(e) => {
                eprintln!("{name}: error: {e}");
                status = ExitCode::FAILURE;
            }
        }
    }
    database.close().await;
    Ok(status)
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
