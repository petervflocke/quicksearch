use gtk4::prelude::*;
use libadwaita as adw;
use crate::search::search_files;
use crate::SearchConfig;
use std::path::PathBuf;
use gio;
use std::thread;
use async_channel;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering}; // Add AtomicUsize here
use gtk4::TextView;
use gdk4;

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

            let text_view: TextView = builder_clone
                .object("text_view")
                .expect("Could not get text_view");
            let buffer = text_view.buffer();

            // Create text tags for clickable paths
            let tag_table = buffer.tag_table();
            let link_tag = gtk4::TextTag::builder()
                .name("link")
                .underline(gtk4::pango::Underline::Single)
                .foreground("blue")
                .build();
            tag_table.add(&link_tag);

            // Add copy icon tag
            let copy_tag = gtk4::TextTag::builder()
                .name("copy")
                .foreground("gray")
                .build();
            tag_table.add(&copy_tag);

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
            let builder_for_click = builder_clone.clone();
            let cancel_button_for_search = cancel_button.clone();
            let status_bar: gtk4::Label = builder_for_click
                .object("status_bar")
                .expect("Could not get status_bar");
            search_button.connect_clicked(move |button| {
                // Reset quit flag
                quit_search.store(false, Ordering::Relaxed);
                // Clear previous results
                buffer.set_text("");
                
                // Update status to "Searching..."
                status_bar.set_label("Searching...");
                
                // Initialize files_processed
                let files_processed = Arc::new(AtomicUsize::new(0));
                let update_status = Arc::new(AtomicBool::new(true)); // Add this line

                // Prepare search config
                let search_path = if path_entry_clone.text().is_empty() {
                    // If no path entered, use current directory
                    std::env::current_dir().unwrap_or_default()
                } else {
                    PathBuf::from(path_entry_clone.text().as_str())
                };

                let search_config = SearchConfig {
                    paths: vec![search_path],
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
                    files_processed: files_processed.clone(), // Add this line
                };

                // Spawn a thread to update the status bar
                let status_bar_clone = status_bar.clone();
                let files_processed_clone = files_processed.clone();
                let update_status_clone = update_status.clone(); // Add this line
                glib::MainContext::default().spawn_local(glib::clone!(@strong status_bar_clone => async move {
                    while update_status_clone.load(Ordering::Relaxed) { // Modify this line
                        let processed = files_processed_clone.load(Ordering::Relaxed);
                        status_bar_clone.set_label(&format!("Files processed: {}", processed));
                        glib::timeout_future(std::time::Duration::from_millis(1000)).await;
                    }
                }));

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
                        update_status.store(false, Ordering::Relaxed); // Add this line
                        match results {
                            Ok(results) => {
                                // Update results in text view
                                for result in &results {
                                    let mut end = buffer_for_results.end_iter();
                                    
                                    // Create mark for copy icon
                                    let copy_start = buffer_for_results.create_mark(None, &end, true);
                                    buffer_for_results.insert(&mut end, "📋 ");
                                    let copy_start_iter = buffer_for_results.iter_at_mark(&copy_start);
                                    buffer_for_results.apply_tag_by_name("copy", &copy_start_iter, &buffer_for_results.end_iter());
                                    buffer_for_results.delete_mark(&copy_start);
                                    
                                    // Create mark for link
                                    let link_start = buffer_for_results.create_mark(None, &buffer_for_results.end_iter(), true);
                                    buffer_for_results.insert(&mut end, &format!("{}:{}\n", result.path.display(), result.line_number));
                                    let link_start_iter = buffer_for_results.iter_at_mark(&link_start);
                                    buffer_for_results.apply_tag_by_name("link", &link_start_iter, &buffer_for_results.end_iter());
                                    buffer_for_results.delete_mark(&link_start);
                                    
                                    // Add the rest of the content
                                    for (line_num, line) in &result.context_before {
                                        buffer_for_results.insert(&mut end, &format!("{:>3} | {}\n", line_num, line));
                                    }
                                    buffer_for_results.insert(&mut end, &format!(">{:>2} | {}\n", result.line_number, result.line));
                                    for (line_num, line) in &result.context_after {
                                        buffer_for_results.insert(&mut end, &format!("{:>3} | {}\n", line_num, line));
                                    }
                                    buffer_for_results.insert(&mut end, "\n");
                                }

                                // Update status bar with result count
                                let processed = files_processed.load(Ordering::Relaxed);
                                status_bar_for_results.set_label(&format!("Found {} matching files, processed {} files", results.len(), processed));
                            },
                            Err(e) => {
                                let mut end = buffer_for_results.end_iter();
                                buffer_for_results.insert(&mut end, &format!("Search error: {}\n", e));
                                let processed = files_processed.load(Ordering::Relaxed);
                            status_bar_for_results.set_label(&format!("Search failed, processed {} files", processed));
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
            let window_for_browse = window.clone();
            let window_for_click = window.clone();
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

                dialog.select_folder(Some(&window_for_browse), None::<&gio::Cancellable>, 
                    glib::clone!(@strong path_entry_for_response => move |result| {
                        if let Ok(folder) = result {
                            if let Some(path) = folder.path() {
                                path_entry_for_response.set_text(path.to_str().unwrap_or(""));
                            }
                        }
                    })
                );
            });

            // Before the motion controller setup
            let link_tag_for_motion = link_tag.clone();
            let motion_controller = gtk4::EventControllerMotion::new();
            motion_controller.connect_motion(move |controller, x, y| {
                if let Ok(view) = controller.widget().downcast::<TextView>() {
                    let (bx, by) = view.window_to_buffer_coords(
                        gtk4::TextWindowType::Widget,
                        x as i32,
                        y as i32,
                    );
                    
                    if let Some(iter) = view.iter_at_location(bx, by) {
                        // Check for clipboard icon click
                        let mut start = iter.clone();
                        start.backward_chars(2);
                        let mut end = iter.clone();
                        end.forward_char();
                        let text = view.buffer().text(&start, &end, false);
                        
                        if text.contains("📋") {
                            let cursor = gdk4::Cursor::from_name("pointer", None).and_then(|c| Some(c));
                            view.set_cursor(cursor.as_ref());
                        } else if iter.has_tag(&link_tag_for_motion) {
                            let cursor = gdk4::Cursor::from_name("pointer", None).and_then(|c| Some(c));
                            view.set_cursor(cursor.as_ref());
                        } else {
                            let cursor = gdk4::Cursor::from_name("text", None).and_then(|c| Some(c));
                            view.set_cursor(cursor.as_ref());
                        }
                    }
                }
            });
            text_view.add_controller(motion_controller);

            // Before the click gesture setup
            let link_tag_for_click = link_tag.clone();
            let click_gesture = gtk4::GestureClick::new();
            click_gesture.connect_pressed(move |gesture, _, x, y| {
                if let Ok(view) = gesture.widget().downcast::<TextView>() {
                    let buffer = view.buffer();
                    let (bx, by) = view.window_to_buffer_coords(
                        gtk4::TextWindowType::Widget,
                        x as i32,
                        y as i32,
                    );
                    
                    if let Some(iter) = view.iter_at_location(bx, by) {
                        // Check for clipboard icon click
                        let mut start = iter.clone();
                        start.backward_chars(2);
                        let mut end = iter.clone();
                        end.forward_char();
                        let text = buffer.text(&start, &end, false);
                        
                        if text.contains("📋") {
                            let path_start = end.clone();
                            let mut path_end = end.clone();
                            path_end.forward_line();
                            let path_text = buffer.text(&path_start, &path_end, false);
                            let clipboard = view.clipboard();
                            clipboard.set_text(&path_text);
                            return;
                        }
                        
                        // Check for file path click
                        if iter.has_tag(&link_tag_for_click) {
                            let mut line_start = iter.clone();
                            line_start.backward_chars(line_start.line_offset());
                            let mut line_end = iter.clone();
                            line_end.forward_line();
                            let path_text = buffer.text(&line_start, &line_end, false);
                            if let Some(path) = path_text.split(':').next() {
                                let clean_path = path.trim_start_matches("📋 ").trim();
                                let file = gio::File::for_path(clean_path);
                                gtk4::FileLauncher::new(Some(&file))
                                    .launch(Some(&window_for_click), None::<&gio::Cancellable>, |_| {});
                            }
                        }
                    }
                }
            });
            text_view.add_controller(click_gesture);
        });
    }

    pub fn run(&self) -> i32 {
        self.app.run_with_args::<&str>(&[]).into()
    }
}