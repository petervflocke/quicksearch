use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

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
            let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            content.append(&header);

            // Add a simple search entry
            let search_entry = gtk4::SearchEntry::new();
            content.append(&search_entry);

            // Add a simple pattern entry
            let pattern_entry = gtk4::Entry::new();
            pattern_entry.set_placeholder_text(Some("File pattern (e.g., *.txt, *.rs)"));
            content.append(&pattern_entry);

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