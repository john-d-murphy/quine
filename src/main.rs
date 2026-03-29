mod commands;
mod db;
mod errors;
mod extract;
mod types;
mod walk;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "quine", about = "A personal knowledge graph.", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Walk, hash, extract, and build the graph from a seed directory.
    Collect {
        /// Path to the seed directory (must contain quine.yaml).
        seed: PathBuf,

        /// Path to the database file.
        #[arg(long, default_value = "quine.db")]
        db: PathBuf,

        /// Print progress during collection.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Search the graph for nodes matching a query. Prints paths to stdout.
    /// Designed for piping to fzf.
    Find {
        /// Search query (matches against file paths).
        query: String,

        /// Path to the database file.
        #[arg(long, default_value = "quine.db")]
        db: PathBuf,
    },

    /// Create a quine.yaml in a directory, making it a root.
    Init {
        /// Directory to initialize.
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Human-readable name for this root.
        #[arg(long)]
        name: Option<String>,
    },

    /// Create a .quine-stop sentinel in a directory.
    Stop {
        /// Directory to stop the walker from entering.
        dir: PathBuf,
    },

    /// Remove a .quine-stop sentinel from a directory.
    Unstop {
        /// Directory to resume walking.
        dir: PathBuf,
    },

    /// Show the discovery tree (all roots and their refs).
    Roots {
        /// Path to the database file.
        #[arg(long, default_value = "quine.db")]
        db: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Collect { seed, db, verbose } => cmd_collect(&seed, &db, verbose),
        Commands::Find { query, db } => cmd_find(&query, &db),
        Commands::Init { dir, name } => cmd_init(&dir, name),
        Commands::Stop { dir } => cmd_stop(&dir),
        Commands::Unstop { dir } => cmd_unstop(&dir),
        Commands::Roots { db } => cmd_roots(&db),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_collect(seed: &Path, db: &Path, verbose: bool) -> errors::Result<()> {
    let report = commands::collect::run(seed, db, verbose)?;
    println!("collect complete:");
    println!("  roots discovered: {}", report.roots_discovered);
    println!("  files added:      {}", report.files_added);
    println!("  files removed:    {}", report.files_removed);
    println!("  files changed:    {}", report.files_changed);
    println!("  files unchanged:  {}", report.files_unchanged);
    println!("  edges added:      {}", report.edges_added);
    if report.warnings > 0 {
        println!("  warnings:         {}", report.warnings);
    }
    if report.broken_edges > 0 {
        println!("  BROKEN EDGES:     {}", report.broken_edges);
    }
    Ok(())
}

fn cmd_find(query: &str, db: &Path) -> errors::Result<()> {
    commands::find::run(query, db)
}

fn cmd_init(dir: &Path, name: Option<String>) -> errors::Result<()> {
    use std::fs;

    let dir_path = types::NodePath::from_cwd(dir).ok_or_else(|| {
        errors::QuineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot resolve path: {}", dir.display()),
        ))
    })?;

    let yaml_path = dir_path.as_path().join(walk::DEFINITION_FILE);
    if yaml_path.exists() {
        eprintln!("quine.yaml already exists at {}", yaml_path.display());
        std::process::exit(1);
    }

    // Derive name from directory name if not provided.
    let root_name = name.unwrap_or_else(|| {
        dir_path
            .as_path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string()
    });

    let content = format!("name: \"{}\"\nrefs: []\n", root_name);
    fs::create_dir_all(dir_path.as_path())?;
    fs::write(&yaml_path, content)?;

    println!("created {} (name: \"{}\")", yaml_path.display(), root_name);
    Ok(())
}

fn cmd_stop(dir: &Path) -> errors::Result<()> {
    use std::fs;

    let dir_path = types::NodePath::from_cwd(dir).ok_or_else(|| {
        errors::QuineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot resolve path: {}", dir.display()),
        ))
    })?;

    let stop_path = dir_path.as_path().join(walk::STOP_FILE);
    if stop_path.exists() {
        eprintln!(".quine-stop already exists at {}", stop_path.display());
        return Ok(());
    }

    fs::create_dir_all(dir_path.as_path())?;
    fs::write(&stop_path, "")?;

    println!("created {}", stop_path.display());
    Ok(())
}

fn cmd_unstop(dir: &Path) -> errors::Result<()> {
    use std::fs;

    let dir_path = types::NodePath::from_cwd(dir).ok_or_else(|| {
        errors::QuineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot resolve path: {}", dir.display()),
        ))
    })?;

    let stop_path = dir_path.as_path().join(walk::STOP_FILE);
    if !stop_path.exists() {
        eprintln!("no .quine-stop at {}", stop_path.display());
        return Ok(());
    }

    fs::remove_file(&stop_path)?;

    println!("removed {}", stop_path.display());
    Ok(())
}

fn cmd_roots(db: &Path) -> errors::Result<()> {
    let db = db::Db::open(db)?;
    let roots = db.list_roots()?;

    if roots.is_empty() {
        println!("no roots found");
    } else {
        for (name, path) in &roots {
            println!("  {:20} {}", name, path);
        }
    }

    Ok(())
}
