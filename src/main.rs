mod analyze;
mod db;
mod generate;
mod models;
mod query;
mod source;

use clap::{Parser, Subcommand};
use switchy_database::{Database, DatabaseValue};

use crate::models::Source;
use crate::source::github::GitHubScraper;
use crate::source::Scraper;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("database error: {0}")]
    Database(#[from] switchy_database::DatabaseError),

    #[error("migration error: {0}")]
    Migration(#[from] switchy_schema::MigrationError),

    #[error("db init error: {0}")]
    DbInit(#[from] switchy_database_connection::InitSqliteRusqliteError),

    #[error("parse error: {0}")]
    Parse(#[from] moosicbox_json_utils::ParseError),

    #[error("config error: {0}")]
    Config(String),

    #[error("scrape error: {0}")]
    Scrape(String),
}

#[derive(Parser)]
#[command(
    name = "tone-clone",
    version,
    about = "Scrape your real writing into a local SQLite database"
)]
struct Cli {
    /// Path to database file (default: ~/.local/share/tone-clone/tone-clone.db)
    #[arg(long, global = true)]
    db: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage scraping sources
    Sources {
        #[command(subcommand)]
        action: SourceAction,
    },
    /// Run a scrape for all sources (or a specific one)
    Scrape {
        /// Only scrape this source ID
        #[arg(long)]
        source_id: Option<i64>,

        /// Resume from last cursor
        #[arg(long)]
        resume: bool,
    },
    /// Show database statistics
    Stats {
        /// Filter to a specific source ID
        #[arg(long)]
        source_id: Option<i64>,
    },
    /// Full-text search across posts
    Query {
        /// FTS5 search terms
        terms: String,

        /// Max results to return
        #[arg(long, default_value = "20")]
        limit: i64,

        /// Exclude posts flagged as likely AI
        #[arg(long)]
        exclude_ai: bool,

        /// Filter to specific post types (comma-separated)
        #[arg(long, value_delimiter = ',')]
        r#type: Option<Vec<String>>,
    },
    /// Generate voice profile and example files from your writing
    Generate {
        /// Output directory (default: ~/.local/share/tone-clone/profiles/)
        #[arg(long)]
        output_dir: Option<String>,

        /// Print to stdout instead of writing files
        #[arg(long)]
        stdout: bool,

        /// Only generate for a specific post type
        #[arg(long)]
        r#type: Option<String>,

        /// FTS search to focus examples on a topic
        #[arg(long)]
        topic: Option<String>,

        /// Max examples per type (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Include posts flagged as likely AI (excluded by default)
        #[arg(long)]
        no_exclude_ai: bool,

        /// Filter to a specific source ID
        #[arg(long)]
        source_id: Option<i64>,
    },
}

#[derive(Subcommand)]
enum SourceAction {
    /// Add a new source
    Add {
        /// Platform name (e.g., github)
        platform: String,

        /// Username on that platform
        username: String,

        /// ISO-8601 date; posts after this are flagged as likely AI-generated
        #[arg(long)]
        ai_cutoff: Option<String>,
    },
    /// List all configured sources
    List,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    let db_path = match cli.db {
        Some(p) => std::path::PathBuf::from(p),
        None => db::default_db_path()?,
    };

    let db = db::open(&db_path).await?;

    match cli.command {
        Commands::Sources { action } => match action {
            SourceAction::Add {
                platform,
                username,
                ai_cutoff,
            } => cmd_source_add(db.as_ref(), &platform, &username, ai_cutoff.as_deref()).await?,
            SourceAction::List => cmd_source_list(db.as_ref()).await?,
        },
        Commands::Scrape { source_id, resume } => {
            cmd_scrape(db.as_ref(), source_id, resume).await?
        }
        Commands::Stats { source_id } => cmd_stats(db.as_ref(), source_id).await?,
        Commands::Query {
            terms,
            limit,
            exclude_ai,
            r#type,
        } => {
            let type_strs: Option<Vec<&str>> = r#type
                .as_ref()
                .map(|v| v.iter().map(|s| s.as_str()).collect());
            cmd_query(db.as_ref(), &terms, limit, exclude_ai, type_strs.as_deref()).await?
        }
        Commands::Generate {
            output_dir,
            stdout,
            r#type,
            topic,
            limit,
            no_exclude_ai,
            source_id,
        } => {
            let mut opts = generate::GenerateOpts::default();
            if let Some(dir) = output_dir {
                opts.output_dir = std::path::PathBuf::from(dir);
            }
            opts.stdout = stdout;
            opts.post_type = r#type;
            opts.topic = topic;
            opts.limit = limit;
            opts.exclude_ai = !no_exclude_ai;
            opts.source_id = source_id;
            generate::run(db.as_ref(), &opts).await?
        }
    }

    Ok(())
}

async fn cmd_source_add(
    db: &dyn Database,
    platform: &str,
    username: &str,
    ai_cutoff: Option<&str>,
) -> Result<(), Error> {
    db.exec_raw_params(
        "INSERT INTO sources (platform, username, ai_cutoff_date) VALUES (?, ?, ?)",
        &[
            DatabaseValue::String(platform.to_string()),
            DatabaseValue::String(username.to_string()),
            match ai_cutoff {
                Some(d) => DatabaseValue::String(d.to_string()),
                None => DatabaseValue::Null,
            },
        ],
    )
    .await?;

    println!("added source: {platform}/{username}");
    if let Some(cutoff) = ai_cutoff {
        println!("  ai cutoff: {cutoff}");
    }
    Ok(())
}

async fn cmd_source_list(db: &dyn Database) -> Result<(), Error> {
    let rows = db.query_raw("SELECT * FROM sources ORDER BY id").await?;

    if rows.is_empty() {
        println!("no sources configured. add one with: tone-clone sources add github <username>");
        return Ok(());
    }

    for row in &rows {
        let source = Source::from_row(row)?;
        let cutoff = source.ai_cutoff_date.as_deref().unwrap_or("none");
        println!(
            "  [{}] {}/{} (ai cutoff: {}, added: {})",
            source.id, source.platform, source.username, cutoff, source.created_at
        );
    }

    Ok(())
}

async fn cmd_scrape(db: &dyn Database, source_id: Option<i64>, resume: bool) -> Result<(), Error> {
    let rows = if let Some(sid) = source_id {
        db.query_raw_params(
            "SELECT * FROM sources WHERE id = ?",
            &[DatabaseValue::Int64(sid)],
        )
        .await?
    } else {
        db.query_raw("SELECT * FROM sources ORDER BY id").await?
    };

    if rows.is_empty() {
        println!("no sources to scrape.");
        return Ok(());
    }

    for row in &rows {
        let source = Source::from_row(row)?;

        // Get resume cursor if requested
        let resume_cursor = if resume {
            let log_rows = db
                .query_raw_params(
                    "SELECT cursor FROM scrape_log WHERE source_id = ? AND cursor IS NOT NULL ORDER BY id DESC LIMIT 1",
                    &[DatabaseValue::Int64(source.id)],
                )
                .await?;
            log_rows.first().and_then(|r| match r.get("cursor") {
                Some(DatabaseValue::String(s)) => Some(s),
                _ => None,
            })
        } else {
            None
        };

        // Create scrape log entry
        db.exec_raw_params(
            "INSERT INTO scrape_log (source_id) VALUES (?)",
            &[DatabaseValue::Int64(source.id)],
        )
        .await?;

        let log_id_rows = db.query_raw("SELECT last_insert_rowid() as id").await?;
        let log_id: i64 = log_id_rows
            .first()
            .and_then(|r| match r.get("id") {
                Some(DatabaseValue::Int64(n)) => Some(n),
                _ => None,
            })
            .unwrap_or(0);

        let scraper: Box<dyn Scraper> = match source.platform.as_str() {
            "github" => Box::new(GitHubScraper::new()),
            other => {
                eprintln!("unknown platform: {other}, skipping");
                continue;
            }
        };

        let result = scraper
            .scrape(db, &source, resume_cursor.as_deref())
            .await?;

        // Update scrape log
        db.exec_raw_params(
            "UPDATE scrape_log SET finished_at = datetime('now'), posts_fetched = ?, cursor = ? WHERE id = ?",
            &[
                DatabaseValue::Int64(result.posts_fetched),
                match &result.cursor {
                    Some(c) => DatabaseValue::String(c.clone()),
                    None => DatabaseValue::Null,
                },
                DatabaseValue::Int64(log_id),
            ],
        )
        .await?;
    }

    Ok(())
}

async fn cmd_stats(db: &dyn Database, source_id: Option<i64>) -> Result<(), Error> {
    let stats = query::stats(db, source_id).await?;

    println!(
        "posts: {} total ({} authentic, {} likely ai)",
        stats.total_posts, stats.authentic_posts, stats.likely_ai_posts
    );

    if !stats.by_type.is_empty() {
        println!("\nby type:");
        for (post_type, count) in &stats.by_type {
            println!("  {post_type}: {count}");
        }
    }

    if !stats.by_source.is_empty() {
        println!("\nby source:");
        for (platform, username, count) in &stats.by_source {
            println!("  {platform}/{username}: {count}");
        }
    }

    Ok(())
}

async fn cmd_query(
    db: &dyn Database,
    terms: &str,
    limit: i64,
    exclude_ai: bool,
    post_types: Option<&[&str]>,
) -> Result<(), Error> {
    let posts = query::search(db, terms, exclude_ai, post_types, limit).await?;

    if posts.is_empty() {
        println!("no results for: {terms}");
        return Ok(());
    }

    println!("{} result(s):\n", posts.len());

    for post in &posts {
        let ai_flag = if post.likely_ai { " [ai]" } else { "" };
        let url = post.url.as_deref().unwrap_or("(no url)");
        println!(
            "--- {} {} {}{}",
            post.post_type, post.created_at, url, ai_flag
        );

        // Truncate long bodies for display
        let body = if post.body.len() > 500 {
            format!("{}...", &post.body[..500])
        } else {
            post.body.clone()
        };
        println!("{body}\n");
    }

    Ok(())
}
