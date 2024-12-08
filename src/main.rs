use clap::Parser;
use anyhow::Result;
use std::env;
use std::path::PathBuf;

mod search;
use search::search_files;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Text to search for
    #[arg(short, long)]
    pub text: String,

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
}

#[derive(Clone)]
pub struct SearchConfig {
    pub paths: Vec<PathBuf>,
    pub patterns: Vec<String>,
    pub query: String,
    pub verbose: bool,
    pub context_lines: usize,
    pub search_binary: bool,
    pub num_workers: usize,
}

impl SearchConfig {
    fn get_search_path(&self) -> String {
        self.paths.first()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
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

fn main() -> Result<()> {
    // Set PDF_QUIET=1 to suppress PDF warnings
    env::set_var("PDF_QUIET", "1");
    
    // Optional: Set RUST_BACKTRACE=1 for debugging
    if env::var("RUST_BACKTRACE").is_err() {
        env::set_var("RUST_BACKTRACE", "0");
    }

    let args = Args::parse();
    
    let config = SearchConfig {
        paths: args.paths,
        patterns: args.pattern.split(',')
            .map(|s| s.trim().to_string())
            .collect(),
        query: args.text,
        verbose: args.verbose,
        context_lines: args.context,
        search_binary: false,
        num_workers: args.workers,
    };

    let results = search_files(&config)?;
    
    for result in results {
        print_search_result(&result);
    }

    Ok(())
}