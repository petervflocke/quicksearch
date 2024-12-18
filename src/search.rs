use anyhow::Result;
use grep::{
    regex::RegexMatcher,
    searcher::{
        Searcher, Sink, SinkMatch, SinkContext, SinkContextKind,
        SearcherBuilder, BinaryDetection, SinkFinish
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
use regex::escape;

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
    last_match: Option<SearchResult>,
}

impl<'a> SearchSink<'a> {
    fn new(tx: &'a Sender<SearchResult>, path: PathBuf, context_lines: usize) -> Self {
        SearchSink {
            tx,
            path,
            context_before: Vec::new(),
            context_after: Vec::new(),
            context_lines,
            last_match: None,
        }
    }

    fn send_last_match(&mut self) {
        if let Some(mut result) = self.last_match.take() {
            result.context_after = self.context_after.iter()
                .enumerate()
                .map(|(i, line)| (
                    result.line_number + i as u64 + 1,
                    line.clone()
                ))
                .collect();
            self.tx.send(result).unwrap();
            self.context_after.clear();
        }
    }
}

impl<'a> Sink for SearchSink<'a> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.send_last_match();

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
            
            self.last_match = Some(result);
            self.context_after.clear();
        }
        Ok(true)
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, Self::Error> {
        if let Ok(line) = String::from_utf8(ctx.bytes().to_vec()) {
            match ctx.kind() {
                SinkContextKind::Before => {
                    self.context_before.push(line.trim().to_string());
                    if self.context_before.len() > self.context_lines {
                        self.context_before.remove(0);
                    }
                }
                SinkContextKind::After => {
                    if self.context_after.len() < self.context_lines {
                        self.context_after.push(line.trim().to_string());
                    }
                }
                SinkContextKind::Other => {}
            }
        }
        Ok(true)
    }

    fn finish(&mut self, _searcher: &Searcher, _: &SinkFinish) -> Result<(), Self::Error> {
        self.send_last_match();
        Ok(())
    }
}

fn search_pdf(path: &std::path::Path, matcher: &RegexMatcher, tx: &Sender<SearchResult>, verbose: bool, context_lines: usize) -> Result<()> {
    let path_buf = path.to_path_buf();
    
    let result = std::panic::catch_unwind(|| {
        let output = Command::new("pdftotext")
            .arg(path.to_str().unwrap())
            .arg("-")
            .arg("-q")
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
        let lines: Vec<&str> = text.lines().collect();
        
        for (line_number, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && matcher.is_match(trimmed.as_bytes())? {
                let line_num = (line_number + 1) as u64;
                
                // Collect context before
                let context_before: Vec<(u64, String)> = lines[line_number.saturating_sub(context_lines)..line_number]
                    .iter()
                    .enumerate()
                    .map(|(i, &l)| (
                        (line_num - (context_lines - i) as u64),
                        l.trim().to_string()
                    ))
                    .collect();

                // Collect context after
                let context_after: Vec<(u64, String)> = lines[line_number + 1..std::cmp::min(line_number + 1 + context_lines, lines.len())]
                    .iter()
                    .enumerate()
                    .map(|(i, &l)| (
                        line_num + i as u64 + 1,
                        l.trim().to_string()
                    ))
                    .collect();

                let result = SearchResult {
                    path: path_buf.clone(),
                    line_number: line_num,
                    line: trimmed.to_string(),
                    context_before,
                    context_after,
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

pub fn search_files(
    config: &SearchConfig,
    quit: Arc<AtomicBool>
) -> Result<Vec<SearchResult>> {
    let results = search(config, quit)?
        .collect::<Vec<SearchResult>>();
    Ok(results)
}

pub fn search(
    config: &SearchConfig,
    quit: Arc<AtomicBool>
) -> Result<impl Iterator<Item = SearchResult>> {
    let (tx, rx) = mpsc::channel();
    let quit = quit.clone();

    // Clone only what we need from config before the thread spawn
    let patterns = config.patterns.clone();
    let search_path = config.get_search_path();
    let query = config.query.clone();
    let use_regex = config.use_regex;  // Get the regex flag

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
        let query = query.clone();

        // Create matcher based on use_regex flag
        let matcher = if use_regex {
            RegexMatcher::new(&query)
        } else {
            RegexMatcher::new(&escape(&query))
        }.unwrap();

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
                    if let Err(e) = search_pdf(path, &matcher, &tx, verbose, context_lines) {
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

                let mut sink = SearchSink::new(&tx, path.to_path_buf(), context_lines);

                if let Err(e) = searcher.search_path(&matcher, path, &mut sink) {
                    if verbose {
                        eprintln!("Error searching {}: {}", path.display(), e);
                    }
                }
            }
        }));
    }

// TODO: Add configuration parameter to control .gitignore behavior
//       - Add bool field to SearchConfig like `respect_gitignore`
//       - Default to false for searching everything
//       - Add bool field to SearchConfig like `respect_gitignore`
//       - Default to false for searching everything
//       - When true, respect .gitignore rules
    let walker = WalkBuilder::new(&search_path)
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
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
