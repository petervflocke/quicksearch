use anyhow::Result;
use grep::{
    regex::RegexMatcher,
    searcher::{
        Searcher, Sink, SinkMatch, SinkContext, SinkContextKind,
        SearcherBuilder, BinaryDetection
    },
    matcher::Matcher,
};
use ignore::{DirEntry, WalkBuilder, WalkState};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Sender},
        Arc, atomic::{AtomicBool, Ordering},
    },
    thread,
    process::Command,
};
use crossbeam_channel;
use crate::SearchConfig;

pub struct SearchResult {
    pub path: PathBuf,
    pub line_number: u64,
    pub line: String,
    pub context_before: Vec<(u64, String)>,
    pub context_after: Vec<(u64, String)>,
}

struct SearchSink<'a> {
    tx: &'a Sender<SearchResult>,
    path: PathBuf,
    context_before: Vec<String>,
    context_after: Vec<String>,
    context_lines: usize,
}

impl<'a> Sink for SearchSink<'a> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        if let Ok(line) = String::from_utf8(mat.bytes().to_vec()) {
            let result = SearchResult { 
                path: self.path.clone(),
                line_number: mat.line_number().unwrap_or(0),
                line: line.trim().to_string(),
                context_before: self.context_before.iter()
                    .enumerate()
                    .map(|(i, line)| (
                        mat.line_number().unwrap_or(0) - (self.context_before.len() - i) as u64,
                        line.clone()
                    ))
                    .collect(),
                context_after: Vec::new(),
            };
            self.tx.send(result).unwrap();
        }
        Ok(true)
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, Self::Error> {
        if let Ok(line) = String::from_utf8(ctx.bytes().to_vec()) {
            match ctx.kind() {
                SinkContextKind::Before => {
                    self.context_before.push(line);
                    if self.context_before.len() > self.context_lines {
                        self.context_before.remove(0);
                    }
                }
                SinkContextKind::After => {
                    self.context_after.push(line);
                }
                SinkContextKind::Other => {}
            }
        }
        Ok(true)
    }
}

fn search_pdf(path: &std::path::Path, matcher: &RegexMatcher, tx: &Sender<SearchResult>, verbose: bool) -> Result<()> {
    let path_buf = path.to_path_buf();
    
    let result = std::panic::catch_unwind(|| {
        let output = Command::new("pdftotext")
            .arg(path.to_str().unwrap())
            .arg("-")  // Output to stdout
            .arg("-q") // Add quiet flag
            .output()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, 
                format!("Failed to run pdftotext: {}", e)))?;

        if !output.status.success() {
            if verbose {
                eprintln!("Failed to process PDF {} (no error message)", path.display());
            }
            return Ok(());
        }

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        
        for (line_number, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && matcher.is_match(trimmed.as_bytes())? {
                let result = SearchResult {
                    path: path_buf.clone(),
                    line_number: (line_number + 1) as u64,
                    line: trimmed.to_string(),
                    context_before: Vec::new(),
                    context_after: Vec::new(),
                };
                tx.send(result).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Failed to send result")
                })?;
            }
        }
        
        Ok(())
    });

    match result {
        Ok(res) => res,
        Err(_) => {
            if verbose {
                eprintln!("Failed to process PDF {} (no error message)", path_buf.display());
            }
            Ok(())
        }
    }
}

pub fn search_files(config: &SearchConfig) -> Result<Vec<SearchResult>> {
    let results = search(config)?
        .collect::<Vec<SearchResult>>();
    Ok(results)
}

pub fn search(config: &SearchConfig) -> Result<impl Iterator<Item = SearchResult>> {
    let (tx, rx) = mpsc::channel();
    let quit = Arc::new(AtomicBool::new(false));

    // Clone the data we need from config before the thread spawn
    let patterns = config.patterns.clone();
    let search_path = config.get_search_path();

    let num_threads = if config.num_workers == 0 {
        thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(2)
    } else {
        config.num_workers
    };

    if config.verbose {
        println!("Using {} worker threads", num_threads);
    }

    let (work_tx, work_rx) = crossbeam_channel::unbounded::<DirEntry>();
    let mut handles = Vec::new();

    // Spawn worker threads
    for _ in 0..num_threads {
        let work_rx = work_rx.clone();
        let tx = tx.clone();
        let quit = quit.clone();
        let matcher = RegexMatcher::new(&config.query).unwrap();
        let verbose = config.verbose;
        let context_lines = config.context_lines;
        let search_binary = config.search_binary;
        
        handles.push(thread::spawn(move || {
            while let Ok(entry) = work_rx.recv() {
                if quit.load(Ordering::Relaxed) {
                    break;
                }

                let path = entry.path();
                
                // Handle PDFs separately
                if path.extension().map_or(false, |ext| ext == "pdf") {
                    if let Err(e) = search_pdf(path, &matcher, &tx, verbose) {
                        if verbose {
                            eprintln!("Error searching PDF {}: {}", path.display(), e);
                        }
                    }
                    continue;
                }

                // Skip if not a regular file
                if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                    continue;
                }

                let mut searcher = SearcherBuilder::new()
                    .binary_detection(if search_binary {
                        BinaryDetection::none()
                    } else {
                        BinaryDetection::quit(b'\x00')
                    })
                    .before_context(context_lines)
                    .after_context(context_lines)
                    .build();

                let mut sink = SearchSink {
                    tx: &tx,
                    path: path.to_path_buf(),
                    context_before: Vec::new(),
                    context_after: Vec::new(),
                    context_lines,
                };

                if let Err(e) = searcher.search_path(&matcher, path, &mut sink) {
                    if verbose {
                        eprintln!("Error searching {}: {}", path.display(), e);
                    }
                }
            }
        }));
    }

    // Start the directory walker
    let walker = WalkBuilder::new(&search_path)  // Use cloned search_path
        .hidden(true)
        .ignore(true)
        .git_ignore(true)
        .build_parallel();

    let quit_walker = quit.clone();
    thread::spawn(move || {
        walker.run(|| {
            let work_tx = work_tx.clone();
            let patterns = patterns.clone();  // Use cloned patterns
            let quit = quit_walker.clone();
            
            Box::new(move |result| {
                if quit.load(Ordering::Relaxed) {
                    return WalkState::Quit;
                }

                let entry = match result {
                    Ok(entry) => entry,
                    Err(_) => return WalkState::Continue,
                };

                // Skip if not a file
                if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                    return WalkState::Continue;
                }

                // Check if file matches any pattern
                let file_name = entry.file_name().to_string_lossy();
                if !patterns.iter().any(|p| {
                    glob::Pattern::new(p).map_or(false, |pat| pat.matches(&file_name))
                }) {
                    return WalkState::Continue;
                }

                // Distribute work to worker threads
                if work_tx.send(entry).is_err() {
                    return WalkState::Quit;
                }

                WalkState::Continue
            })
        });

        // Signal workers to stop
        drop(work_tx);
        
        // Wait for workers to finish
        for handle in handles {
            let _ = handle.join();
        }
    });

    Ok(rx.into_iter())
}
