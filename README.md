# QuickSearch

A fast and user-friendly search utility written in Rust, featuring both CLI and GUI interfaces.

## Features

### Command Line Interface (CLI)
- Fast text search in files and directories
- Regular expression support
- Context lines display (before/after match)
- Binary file filtering
- Parallel processing with configurable worker threads

### Graphical Interface (GUI)
- Interactive search with real-time results
- File path copying to clipboard (click 📋 icon)
- Clickable file paths (opens file in default application)
- Context lines display
- Search cancellation support

## Usage

### Command Line Options Examples

```bash
#Search in current directory with GUI
quicksearch -i .
#Search for "pattern" in all .rs files with 2 lines of context
quicksearch -t "pattern" -p ".rs" -c 2 ./src
#Search with 4 worker threads
quicksearch -t "pattern" -j 4 ./src
```

## Project Structure

### Branches
- `main` (v0.1.0-debug-reference)
  - Original version with debug prints
  - Full GUI functionality
  - Reference implementation for debugging

- `feature/clean-gui` (v0.1.0-clean)
  - Clean version without debug prints
  - Enhanced GUI features:
    - Clipboard integration
    - Clickable file paths
    - Improved cursor feedback

### Key Components
- CLI processing (`src/cli.rs`)
- Search engine (`src/search.rs`)
- GUI interface (`src/gui.rs`)
- Configuration handling (`src/config.rs`)

## Building

### Build debug version
```bash
cargo build
```

### Build release version
```bash
cargo build --release
```

### Run debug version
```bash     
./target/debug/quicksearch [OPTIONS] [PATHS]...
```

### Run release version
```bash
./target/release/quicksearch [OPTIONS] [PATHS]...
```

Note: When using GUI mode (-i), run from the project directory to ensure UI resources are found.

## Development

### Requirements
- Rust 1.70 or later
- GTK4 development libraries
- Cargo and standard Rust toolchain

### Dependencies
- GTK4 for GUI
- Regular expressions (regex crate)
- Command line parsing (clap crate)
- Parallel processing (rayon crate)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

