use clap::Parser;
use anyhow::Result;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};

mod search;
mod gui;

use search::{search_files, SearchResult};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Launch interactive mode
    #[arg(short, long, default_value_t = false)]
    pub interactive: bool,

    /// Text to search for
    #[arg(short, long, required = false)]
    pub text: Option<String>,

    /// File pattern to search in (e.g., "*.txt" or "*.{txt,md}")
    #[arg(short, long, default_value = "*")]
    pub pattern: String,

    /// Number of worker threads (default: automatic based on CPU cores)
    #[arg(short = 'j', long = "jobs", default_value = "0")]
    pub workers: usize,

    /// Show verbose output including error messages
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    /// Paths to search in
    pub paths: Vec<PathBuf>,

    /// Number of context lines before and after matches
    #[arg(short = 'c', long = "context", default_value_t = 0)]
    pub context: usize,

    /// Use regex pattern for search
    #[arg(short = 'r', long = "regex", default_value_t = false)]
    pub use_regex: bool,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub paths: Vec<PathBuf>,
    pub patterns: Vec<String>,
    pub query: String,
    pub verbose: bool,
    pub context_lines: usize,
    pub search_binary: bool,
    pub num_workers: usize,
    pub use_regex: bool,
    pub files_processed: Arc<AtomicUsize>, // New field
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            patterns: Vec::new(),
            query: String::new(),
            num_workers: 0,
            context_lines: 0,
            search_binary: false,
            verbose: false,
            use_regex: false,
            files_processed: Arc::new(AtomicUsize::new(0)), // Initialize new field
        }
    }
}

impl SearchConfig {
    fn get_search_path(&self) -> String {
        self.paths.first()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    }

    fn from_args(args: &Args, text: String) -> Self {
        Self {
            paths: args.paths.clone(),
            patterns: vec![args.pattern.clone()],
            query: text,
            verbose: args.verbose,
            context_lines: args.context,
            search_binary: false,
            num_workers: args.workers,
            use_regex: args.use_regex,
            files_processed: Arc::new(AtomicUsize::new(0)), // Initialize new field
        }
    }
}

fn print_search_result(result: &SearchResult) {
    println!("File: {}:{}", result.path.display(), result.line_number);
    
    // Print context before
    for (line_num, line) in &result.context_before {
        println!("{:>3} | {}", line_num, line);
    }
    
    // Print matching line with '>' indicator
    println!(">{:>2} | {}", result.line_number, result.line);
    
    // Print context after
    for (line_num, line) in &result.context_after {
        println!("{:>3} | {}", line_num, line);
    }
    
    // Empty line between files
    println!();
}

fn run_cli(mut args: Args) -> Result<()> {
    let quit = Arc::new(AtomicBool::new(false));
    // Take ownership of text before borrowing args
    let text = args.text.take().unwrap_or_default();
    let config = SearchConfig::from_args(&args, text);
    let results = search_files(&config, quit)?;
    
    for result in results {
        print_search_result(&result);
    }

    Ok(())
}

fn run_gui(config: SearchConfig) -> Result<()> {
    let gui = gui::SearchGUI::new();
    gui.build_with_config(config);
    gui.run();
    Ok(())
}

fn main() -> Result<()> {
    // Set environment variables
    env::set_var("PDF_QUIET", "1");
    if env::var("RUST_BACKTRACE").is_err() {
        env::set_var("RUST_BACKTRACE", "0");
    }

    // Parse arguments
    let args = Args::parse();

    // Get search text (required for both modes)
    let text = args.text.clone().unwrap_or_default();

    let config = SearchConfig::from_args(&args, text);

    // Choose mode based on interactive flag
    if args.interactive {
        run_gui(config)
    } else {
        run_cli(args)
    }
}