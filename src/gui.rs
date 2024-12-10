use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use crate::search::search_files;
use crate::SearchConfig;
use std::path::PathBuf;

pub struct SearchGUI {
    pub app: adw::Application,
}

impl SearchGUI {
    pub fn new() -> Self {
        // Initialize libadwaita
        adw::init().expect("Failed to initialize libadwaita");

        Self {
            app: adw::Application::builder()
                .application_id("org.quicksearch.app")
                .build()
        }
    }

    pub fn build(&self) {
        self.app.connect_activate(move |app| {
            // Create a new window
            let window = adw::ApplicationWindow::builder()
                .application(app)
                .default_width(800)
                .default_height(600)
                .title("QuickSearch")
                .build();

            // Create a header bar
            let header = adw::HeaderBar::new();
            
            // Create main content box
            let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
            content.set_margin_top(8);
            content.set_margin_bottom(8);
            content.set_margin_start(8);
            content.set_margin_end(8);
            content.append(&header);

            // Add path selection box
            let path_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let path_entry = gtk4::Entry::new();
            path_entry.set_placeholder_text(Some("Search path (e.g., /home/user/docs)"));
            path_entry.set_hexpand(true);
            let path_button = gtk4::Button::with_label("Browse");
            path_box.append(&path_entry);
            path_box.append(&path_button);
            content.append(&path_box);

            // Connect path button
            let path_entry_clone = path_entry.clone();
            let window_clone = window.clone();
            path_button.connect_clicked(move |_| {
                let file_chooser = gtk4::FileDialog::builder()
                    .title("Choose Search Directory")
                    .modal(true)
                    .build();

                let path_entry = path_entry_clone.clone();
                file_chooser.select_folder(
                    Some(&window_clone),
                    None::<&gio::Cancellable>,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                path_entry.set_text(&path.to_string_lossy());
                            }
                        }
                    },
                );
            });

            // Add search entry
            let search_entry = gtk4::SearchEntry::new();
            search_entry.set_placeholder_text(Some("Enter text to search for..."));
            content.append(&search_entry);

            // Add pattern entry
            let pattern_entry = gtk4::Entry::new();
            pattern_entry.set_placeholder_text(Some("File pattern (e.g., *.txt, *.rs)"));
            content.append(&pattern_entry);

            // Add search button
            let search_button = gtk4::Button::with_label("Search");
            content.append(&search_button);

            // Add scrolled window for results
            let scrolled_window = gtk4::ScrolledWindow::new();
            scrolled_window.set_vexpand(true);
            
            // Add text view for results
            let text_view = gtk4::TextView::new();
            text_view.set_editable(false);
            text_view.set_wrap_mode(gtk4::WrapMode::Word);
            scrolled_window.set_child(Some(&text_view));
            content.append(&scrolled_window);

            // Connect search button click
            let text_buffer = text_view.buffer();
            search_button.connect_clicked(move |_| {
                let search_text = search_entry.text().to_string();
                let pattern = pattern_entry.text().to_string();
                let search_path = path_entry.text().to_string();
                
                if search_text.is_empty() {
                    text_buffer.set_text("Please enter text to search for");
                    return;
                }

                // Create search config
                let config = SearchConfig {
                    paths: vec![PathBuf::from(if search_path.is_empty() { "." } else { &search_path })],
                    patterns: pattern.split(',')
                        .map(|s| s.trim().to_string())
                        .collect(),
                    query: search_text,
                    verbose: false,
                    context_lines: 2,
                    search_binary: false,
                    num_workers: 0,
                };

                // Perform search
                match search_files(&config) {
                    Ok(results) => {
                        if results.is_empty() {
                            text_buffer.set_text("No results found");
                        } else {
                            let mut output = String::new();
                            for result in results {
                                output.push_str(&format!("File: {}:{}\n", 
                                    result.path.display(), result.line_number));
                                
                                // Add context before
                                for (line_num, line) in &result.context_before {
                                    output.push_str(&format!("{:>3} | {}\n", line_num, line));
                                }
                                
                                // Add matching line
                                output.push_str(&format!(">{:>2} | {}\n", 
                                    result.line_number, result.line));
                                
                                // Add context after
                                for (line_num, line) in &result.context_after {
                                    output.push_str(&format!("{:>3} | {}\n", line_num, line));
                                }
                                
                                output.push('\n');
                            }
                            text_buffer.set_text(&output);
                        }
                    },
                    Err(e) => {
                        text_buffer.set_text(&format!("Search error: {}", e));
                    }
                }
            });

            // Set the window content
            window.set_content(Some(&content));

            // Show the window
            window.present();
        });
    }

    pub fn run(&self) -> i32 {
        // Run with empty arguments array to avoid GTK argument parsing
        self.app.run_with_args::<&str>(&[]).into()
    }
} 