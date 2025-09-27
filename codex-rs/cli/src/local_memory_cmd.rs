use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use rusqlite::{Connection, Result as SqlResult, Row};
use serde::Serialize;
use serde_yaml::Value as YamlValue;

#[derive(Debug, Parser)]
pub struct LocalMemoryCli {
    #[command(subcommand)]
    cmd: LocalMemorySubcommand,
}

#[derive(Debug, Subcommand)]
enum LocalMemorySubcommand {
    /// Export memories to JSON Lines
    Export(ExportArgs),
}

#[derive(Debug, Parser)]
struct ExportArgs {
    /// Optional output file (defaults to stdout)
    #[arg(long = "output", short = 'o')]
    output: Option<PathBuf>,

    /// Optional override for the local-memory database path
    #[arg(long = "database")]
    database: Option<PathBuf>,

    /// Pretty-print JSON output
    #[arg(long = "pretty")]
    pretty: bool,
}

#[derive(Debug, Serialize)]
struct MemoryExportRecord {
    id: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    importance: i64,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
}

impl LocalMemoryCli {
    pub async fn run(self) -> Result<()> {
        match self.cmd {
            LocalMemorySubcommand::Export(args) => run_export(args).await,
        }
    }
}

async fn run_export(args: ExportArgs) -> Result<()> {
    let db_path = resolve_database_path(args.database.as_ref())?;
    let conn = Connection::open(&db_path).with_context(|| {
        format!(
            "failed to open local-memory database at {}",
            db_path.display()
        )
    })?;

    let mut stmt = conn.prepare(
        "SELECT id, content, source, importance, tags, session_id, domain, created_at, updated_at, agent_type, agent_context, access_scope, slug FROM memories ORDER BY created_at",
    )?;

    let records = stmt.query_map([], |row| row_to_export(row))?;

    let output_path = args.output.clone();
    let mut writer: Box<dyn Write> = match &args.output {
        Some(path) => {
            let file = File::create(path)
                .with_context(|| format!("failed to create output file {}", path.display()))?;
            Box::new(BufWriter::new(file))
        }
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let mut count: usize = 0;
    for record in records {
        let record = record?;
        if args.pretty {
            serde_json::to_writer_pretty(&mut writer, &record)?;
            writeln!(&mut writer)?;
        } else {
            serde_json::to_writer(&mut writer, &record)?;
            writeln!(&mut writer)?;
        }
        count += 1;
    }

    writer.flush()?;

    if output_path.is_none() {
        eprintln!("Exported {count} memories to stdout");
    } else if let Some(path) = output_path {
        eprintln!("Exported {count} memories to {}", path.display());
    }

    Ok(())
}

fn row_to_export(row: &Row<'_>) -> SqlResult<MemoryExportRecord> {
    let tags_raw: String = row.get("tags")?;
    let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_else(|_| vec![tags_raw]);

    Ok(MemoryExportRecord {
        id: row.get("id")?,
        content: row.get("content")?,
        source: row.get("source").ok(),
        importance: row.get("importance")?,
        tags,
        session_id: row.get("session_id").ok(),
        domain: row.get("domain").ok(),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        agent_type: row.get("agent_type").ok(),
        agent_context: row.get("agent_context").ok(),
        access_scope: row.get("access_scope").ok(),
        slug: row.get("slug").ok(),
    })
}

fn resolve_database_path(explicit: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.clone());
    }

    if let Some(home) = std::env::var_os("LOCAL_MEMORY_HOME") {
        let home_path = PathBuf::from(home);
        let candidate = home_path.join("unified-memories.db");
        if candidate.exists() {
            return Ok(candidate);
        }
        let config_path = home_path.join("config.yaml");
        if config_path.exists() {
            if let Some(path) = parse_database_from_config(&config_path) {
                return Ok(path);
            }
        }
    }

    let default_home = default_local_memory_home()?;
    let config_path = default_home.join("config.yaml");
    if config_path.exists() {
        if let Some(path) = parse_database_from_config(&config_path) {
            return Ok(path);
        }
    }

    let fallback = default_home.join("unified-memories.db");
    if fallback.exists() {
        Ok(fallback)
    } else {
        Err(anyhow!(
            "local-memory database not found. Looked for {}",
            fallback.display()
        ))
    }
}

fn parse_database_from_config(config_path: &Path) -> Option<PathBuf> {
    let data = std::fs::read_to_string(config_path).ok()?;
    let value: YamlValue = serde_yaml::from_str(&data).ok()?;
    let db_path = value
        .get("database")?
        .get("path")?
        .as_str()
        .map(PathBuf::from)?;
    Some(db_path)
}

fn default_local_memory_home() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|p| p.join(".local-memory"))
        .ok_or_else(|| anyhow!("failed to determine home directory"))
}
