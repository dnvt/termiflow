//! TermiFlow CLI - Interactive TUI graph explorer
//!
//! "jq for diagrams" - visualize Mermaid flowcharts in your terminal

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::io::IsTerminal;
use std::path::PathBuf;

// Use the termiflow library
use termiflow::{parse, waterfall, render_canvas, Config, BorderStyle};

/// Interactive TUI graph explorer - jq for diagrams
#[derive(Parser)]
#[command(name = "termiflow")]
#[command(version, about, long_about = None)]
#[command(after_help = "Examples:
  termiflow diagram.md              Interactive mode
  termiflow --print diagram.md      Output to stdout
  cat file.md | termiflow --print   Read from stdin
  termiflow -s unicode diagram.md   Use Unicode borders")]
pub struct Cli {
    /// Input Mermaid file (reads from stdin if omitted)
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Border style
    #[arg(short, long, default_value = "unicode", value_enum)]
    pub style: StyleArg,

    /// Output to stdout (no interactive TUI)
    #[arg(long)]
    pub print: bool,

    /// Maximum label width before truncation
    #[arg(long, default_value = "20")]
    pub max_label: usize,

    /// Exit with error on any parse warning
    #[arg(long)]
    pub strict: bool,

    /// Dump layout coordinates (debugging)
    #[arg(long, hide = true)]
    pub debug_layout: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
#[derive(Default)]
pub enum StyleArg {
    Ascii,
    #[default]
    Unicode,
    Double,
    Rounded,
    Heavy,
}


impl From<StyleArg> for BorderStyle {
    fn from(arg: StyleArg) -> Self {
        match arg {
            StyleArg::Ascii => BorderStyle::Ascii,
            StyleArg::Unicode => BorderStyle::Unicode,
            StyleArg::Double => BorderStyle::Double,
            StyleArg::Rounded => BorderStyle::Rounded,
            StyleArg::Heavy => BorderStyle::Heavy,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // --print mode: safe, no raw terminal, works with any stdin/stdout
    if cli.print {
        return run_print_mode(&cli);
    }

    // TUI mode: stdout MUST be TTY (need raw mode for rendering)
    if !std::io::stdout().is_terminal() {
        eprintln!("termiflow: error: Interactive mode requires terminal stdout.");
        eprintln!("  Hint: Use --print for piped output");
        eprintln!("  Example: termiflow --print diagram.md > output.txt");
        std::process::exit(1);
    }

    // TUI mode: stdin pipe without file arg is ambiguous
    if !std::io::stdin().is_terminal() && cli.file.is_none() {
        eprintln!("termiflow: error: Cannot read from stdin pipe in TUI mode.");
        eprintln!("  Hint: Provide a file argument or use --print");
        eprintln!("  Example: cat diagram.md | termiflow --print");
        std::process::exit(1);
    }

    // Check for Unicode capability
    if cli.style != StyleArg::Ascii && !supports_unicode() {
        eprintln!("termiflow: warning: Unicode may not display correctly");
        eprintln!(
            "  Terminal: {}",
            std::env::var("TERM").unwrap_or_default()
        );
        eprintln!("  Hint: Use --style ascii for maximum compatibility");
    }

    run_tui_mode(&cli)
}

fn supports_unicode() -> bool {
    // Check LANG/LC_ALL for UTF-8
    let lang_ok = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .map(|v| v.to_uppercase().contains("UTF"))
        .unwrap_or(false);

    // Check TERM for known-good terminals
    let term_ok = std::env::var("TERM")
        .map(|v| v.contains("256color") || v.contains("xterm") || v == "screen")
        .unwrap_or(false);

    lang_ok || term_ok
}

fn run_print_mode(cli: &Cli) -> Result<()> {
    // Read input
    let input = read_input(cli)?;

    // Parse the Mermaid content (returns ParseResult with graph + in-file config)
    let parse_result = parse(&input, cli.strict)?;

    // Load configuration (CLI > in-file > config file)
    let config = Config::builder()
        .max_label_width(cli.max_label)
        .strict(cli.strict)
        .build(&parse_result.config);

    // Run layout algorithm (may add warnings)
    let graph = waterfall(parse_result.graph)?;

    // Print any warnings to stderr (parser + layout)
    for warning in &graph.warnings {
        eprintln!("{}", warning);
    }

    // Optional debug: dump layout coordinates
    if cli.debug_layout {
        eprintln!("-- layout --");
        for n in &graph.nodes {
            eprintln!(
                "node {}: label='{}' x={} y={} w={} rank={}",
                n.id, n.label, n.x, n.y, n.width, n.rank
            );
        }
        for e in &graph.edges {
            eprintln!(
                "edge {} -> {} (back_edge={})",
                e.from, e.to, e.is_back_edge
            );
        }
    }

    // Render to canvas
    let output = render_canvas(&graph, &config)?;

    // Print to stdout
    print!("{}", output);

    Ok(())
}

fn run_tui_mode(_cli: &Cli) -> Result<()> {
    // TODO: Implement TUI mode on Day 4
    eprintln!("termiflow: TUI mode not yet implemented. Use --print for now.");
    std::process::exit(1);
}

fn read_input(cli: &Cli) -> Result<String> {
    use std::io::Read;

    match &cli.file {
        Some(path) => {
            std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))
        }
        None => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            Ok(input)
        }
    }
}
