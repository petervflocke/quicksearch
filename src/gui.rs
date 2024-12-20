use gtk4::prelude::*;
use libadwaita as adw;
use crate::search::search_files;
use crate::SearchConfig;
use std::path::PathBuf;
use gio;
use std::thread;
use async_channel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SearchGUI {
    pub app: adw::Application,
    builder: gtk4::Builder,
}

impl SearchGUI {
    pub fn new() -> Self {
        // Initialize libadwaita
        adw::init().expect("Failed to initialize libadwaita");

        // Create builder and load UI file
        let builder = gtk4::Builder::from_file("src/ui/windows.ui");
        
        // Verify that we can load all required widgets
        let required_widgets = ["main_window", "path_entry", "search_entry", 
                              "pattern_entry", "number_processes", "number_lines",
                              "search_button", "browse_button"];
        
        for widget in required_widgets {
            if builder.object::<gtk4::Widget>(widget).is_none() {
                panic!("Could not find required widget '{}' in UI file", widget);
            }
        }

        Self {
            app: adw::Application::builder()
                .application_id("org.quicksearch.app")
                .build(),
            builder,
        }
    }

    pub fn build_with_config(&self, config: SearchConfig) {
        // Debug prints commented out for cleaner output, uncomment if needed for debugging
        println!("GUI received config: {:?}", config);
        let builder_clone = self.builder.clone();
        let config_clone = config.clone();
        
        self.app.connect_activate(move |app| {
            // println!("Setting initial values from config: {:?}", config_clone);
            
            let window: gtk4::Window = builder_clone
                .object("main_window")
                .expect("Could not get main_window");
            window.set_application(Some(app));
            window.present();

            // Get all widgets
            let path_entry: gtk4::Entry = builder_clone
                .object("path_entry")
                .expect("Could not get path_entry");
            
            let search_entry: gtk4::SearchEntry = builder_clone
                .object("search_entry")
                .expect("Could not get search_entry");
            
            let pattern_entry: gtk4::Entry = builder_clone
                .object("pattern_entry")
                .expect("Could not get pattern_entry");
            
            let number_processes: gtk4::SpinButton = builder_clone
                .object("number_processes")
                .expect("Could not get number_processes");

            let number_lines: gtk4::Entry = builder_clone
                .object("number_lines")
                .expect("Could not get number_lines");

            let results_view: gtk4::TextView = builder_clone
                .object("results_view")
                .expect("Could not get results_view");
            
            let buffer = results_view.buffer();

            // Get regex checkbox
            let regex_checkbox: gtk4::CheckButton = builder_clone
                .object("regex-onoff")
                .expect("Could not get regex checkbox");

            // Set initial regex state from config
            regex_checkbox.set_active(config_clone.use_regex);

            // Set initial values from config
            if !config_clone.paths.is_empty() {
                path_entry.set_text(&config_clone.paths[0].to_string_lossy());
            }
            search_entry.set_text(&config_clone.query);
            pattern_entry.set_text(&config_clone.patterns.join(","));
            
            // Fix: Properly set the SpinButton value and range
            number_processes.set_range(0.0, 32.0);  // Allow 0 for auto-detection
            number_processes.set_increments(1.0, 4.0);  // Step by 1, page by 4
            number_processes.set_value(config_clone.num_workers as f64);

            // Add tooltip to explain 0
            number_processes.set_tooltip_text(Some("Number of worker threads (0 = automatic)"));

            number_lines.set_text(&config_clone.context_lines.to_string());

            // Connect search button
            let quit_search = Arc::new(AtomicBool::new(false));

            // Get both buttons
            let search_button: gtk4::Button = builder_clone
                .object("search_button")
                .expect("Could not get search_button");
            let cancel_button: gtk4::Button = builder_clone
                .object("cancel_button")
                .expect("Could not get cancel_button");

            // Set up cancel button handler
            let quit_search_for_cancel = quit_search.clone();
            cancel_button.connect_clicked(move |button| {
                quit_search_for_cancel.store(true, Ordering::Relaxed);
                button.set_sensitive(false);
            });

            let path_entry_clone = path_entry.clone();
            let search_entry_clone = search_entry.clone();
            let pattern_entry_clone = pattern_entry.clone();
            let number_processes_clone = number_processes.clone();
            let number_lines_clone = number_lines.clone();
            let regex_checkbox_clone = regex_checkbox.clone();

            // Modify search button handler
            let builder_for_click = builder_clone.clone();
            let cancel_button_for_search = cancel_button.clone();
            search_button.connect_clicked(move |button| {
                // Reset quit flag
                quit_search.store(false, Ordering::Relaxed);
                
                // Get status bar
                let status_bar: gtk4::Label = builder_for_click
                    .object("status_bar")
                    .expect("Could not get status_bar");
                
                // Clear previous results
                buffer.set_text("");
                
                // Update status to "Searching..."
                status_bar.set_label("Searching...");
                
                // Prepare search config
                let search_path = if path_entry_clone.text().is_empty() {
                    // If no path entered, use current directory
                    std::env::current_dir().unwrap_or_default()
                } else {
                    PathBuf::from(path_entry_clone.text().as_str())
                };

                let search_config = SearchConfig {
                    paths: vec![search_path],  // Use the processed path
                    patterns: pattern_entry_clone.text()
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect(),
                    query: search_entry_clone.text().to_string(),
                    num_workers: number_processes_clone.value() as usize,
                    context_lines: number_lines_clone.text()
                        .as_str()
                        .parse()
                        .unwrap_or(0),
                    verbose: false,
                    search_binary: false,
                    use_regex: regex_checkbox_clone.is_active(),
                };
                
                // Disable search button, enable cancel button
                button.set_sensitive(false);
                cancel_button_for_search.set_sensitive(true);
                
                // Create channel for search results
                let (tx, rx) = async_channel::bounded(1);
                
                // Prepare clones for the search thread
                let quit_search_for_thread = quit_search.clone();
                let search_config_for_thread = search_config.clone();
                let tx_for_thread = tx.clone();

                // Prepare clones for the results handler
                let buffer_for_results = buffer.clone();
                let status_bar_for_results = status_bar.clone();
                let button_for_results = button.clone();
                let cancel_button_for_results = cancel_button_for_search.clone();

                // Spawn search thread
                thread::spawn(move || {
                    let results = search_files(&search_config_for_thread, quit_search_for_thread);
                    let _ = tx_for_thread.try_send(results);
                });

                // Handle results
                glib::spawn_future_local(async move {
                    if let Ok(results) = rx.recv().await {
                        match results {
                            Ok(results) => {
                                // Update results in text view
                                for result in &results {
                                    let mut text = format!("File: {}:{}\n", result.path.display(), result.line_number);
                                    
                                    for (line_num, line) in &result.context_before {
                                        text.push_str(&format!("{:>3} | {}\n", line_num, line));
                                    }
                                    
                                    text.push_str(&format!(">{:>2} | {}\n", result.line_number, result.line));
                                    
                                    for (line_num, line) in &result.context_after {
                                        text.push_str(&format!("{:>3} | {}\n", line_num, line));
                                    }
                                    
                                    text.push('\n');
                                    
                                    let mut end = buffer_for_results.end_iter();
                                    buffer_for_results.insert(&mut end, &text);
                                }

                                // Update status bar with result count
                                status_bar_for_results.set_label(&format!("Found {} matching files", results.len()));
                            },
                            Err(e) => {
                                let mut end = buffer_for_results.end_iter();
                                buffer_for_results.insert(&mut end, &format!("Search error: {}\n", e));
                                status_bar_for_results.set_label("Search failed");
                            }
                        }
                        
                        // Re-enable search button, disable cancel button
                        button_for_results.set_sensitive(true);
                        cancel_button_for_results.set_sensitive(false);
                    }
                });
            });

            // Connect browse button
            let browse_button: gtk4::Button = builder_clone
                .object("browse_button")
                .expect("Could not get browse_button");
            
            let path_entry_clone = path_entry.clone();
            let window_clone = window.clone();
            browse_button.connect_clicked(move |_| {
                let dialog = gtk4::FileDialog::builder()
                    .title("Select Directory")
                    .modal(true)
                    .build();

                // Set the initial folder based on the current path entry content
                let current_path = path_entry_clone.text().to_string();
                let path_entry_for_response = path_entry_clone.clone();

                if !current_path.is_empty() {
                    let initial_folder = gio::File::for_path(current_path);
                    dialog.set_initial_folder(Some(&initial_folder));
                }

                dialog.select_folder(Some(&window_clone), None::<&gio::Cancellable>, 
                    glib::clone!(@strong path_entry_for_response => move |result| {
                        if let Ok(folder) = result {
                            if let Some(path) = folder.path() {
                                path_entry_for_response.set_text(path.to_str().unwrap_or(""));
                            }
                        }
                    })
                );
            });
        });
    }

    pub fn run(&self) -> i32 {
        self.app.run_with_args::<&str>(&[]).into()
    }
} 