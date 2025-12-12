//! TermiFlow CLI implementation.
//!
//! This module is shared by both binary entrypoints:
//! - `termiflow`
//! - `tw` (short alias)

use anyhow::Result;
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;

// Use the termiflow library
use termiflow::{layout, measure, parse, render_canvas, CompositeStyle, Config};

/// Interactive TUI graph explorer - jq for diagrams
#[derive(Parser)]
#[command(name = "termiflow")]
#[command(version, about, long_about = None)]
#[command(after_help = "Examples:
  termiflow diagram.md              Print to stdout (default)
  termiflow -f diagram.md           File flag (jq-style)
  termiflow --print diagram.md      Print (explicit)
  termiflow --tui diagram.md        Interactive mode (not yet implemented)
  cat file.md | termiflow           Read from stdin
  termiflow -s unicode diagram.md   Use Unicode borders")]
pub struct Cli {
    /// Input Mermaid file (reads from stdin if omitted)
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Input Mermaid file (flag form, jq-style parity)
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pub file_flag: Option<PathBuf>,

    /// Border style: simple (ascii, unicode, double, rounded, heavy, dots, plus, stars, blocks)
    /// or composite (corner:rounded,border:heavy,arrow:unicode)
    #[arg(short, long, value_name = "STYLE")]
    pub style: Option<String>,

    /// Output to stdout (no interactive TUI)
    #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "-")]
    pub print: Option<PathBuf>,

    /// Interactive mode (not yet implemented)
    #[arg(long)]
    pub tui: bool,

    /// Maximum label width before truncation
    #[arg(long, default_value = "20")]
    pub max_label: usize,

    /// Enable multiline label wrapping (experimental; default off)
    #[arg(long)]
    pub wrap: bool,

    /// Maximum number of label lines when wrapping is enabled
    #[arg(long, value_name = "N")]
    pub max_lines: Option<usize>,

    /// Crop empty margins around output (enabled by default; use --no-crop to disable)
    #[arg(long)]
    pub crop: bool,

    /// Disable cropping empty margins around output
    #[arg(long = "no-crop")]
    pub no_crop: bool,

    /// Add padding (in spaces/lines) around output
    #[arg(long, value_name = "N")]
    pub pad: Option<usize>,

    /// Use a tighter layout spacing (less whitespace)
    #[arg(long)]
    pub compact: bool,

    /// Exit with error on any parse warning
    #[arg(long)]
    pub strict: bool,

    /// Dump layout coordinates (debugging)
    #[arg(long, hide = true)]
    pub debug_layout: bool,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    // Default mode is print-to-stdout (jq-style).
    // `--tui` opts into the interactive mode (not implemented yet).
    if cli.tui {
        // TUI mode: stdout MUST be TTY (need raw mode for rendering)
        if !std::io::stdout().is_terminal() {
            eprintln!("termiflow: error: Interactive mode requires terminal stdout.");
            eprintln!("  Hint: Use print mode for piped output");
            eprintln!("  Example: termiflow diagram.md > output.txt");
            std::process::exit(1);
        }

        // TUI mode: stdin pipe without file arg is ambiguous
        if !std::io::stdin().is_terminal() && cli.input_path().is_none() {
            eprintln!("termiflow: error: Cannot read from stdin pipe in TUI mode.");
            eprintln!("  Hint: Provide a file argument or use print mode");
            std::process::exit(1);
        }
    } else {
        return run_print_mode(&cli);
    }

    // Check for Unicode capability (skip check if using ASCII)
    let is_ascii = cli.style.as_deref() == Some("ascii");
    if !is_ascii && !supports_unicode() {
        eprintln!("termiflow: warning: Unicode may not display correctly");
        eprintln!("  Terminal: {}", std::env::var("TERM").unwrap_or_default());
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
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    let debug_routes = std::env::var("TERMIFLOW_DEBUG_ROUTES").is_ok();
    let t0 = std::time::Instant::now();

    // Read input
    let input = read_input(cli)?;

    // Parse the Mermaid content (returns ParseResult with graph + in-file config)
    let parse_result = parse(&input, cli.strict)?;
    if debug_timing {
        eprintln!("termiflow: parse {:?}", t0.elapsed());
    }

    // Load configuration (CLI > in-file > config file)
    let mut builder = Config::builder()
        .max_label_width(cli.max_label)
        .wrap_labels(cli.wrap)
        .strict(cli.strict);

    if let Some(n) = cli.max_lines {
        builder = builder.max_label_lines(n);
    }

    if cli.no_crop {
        builder = builder.crop(false);
    } else if cli.crop {
        builder = builder.crop(true);
    }

    if let Some(pad) = cli.pad {
        builder = builder.pad(pad);
    }

    // Only apply style if explicitly provided on CLI
    if let Some(ref style_str) = cli.style {
        builder = builder.style(CompositeStyle::parse(style_str));
    }

    let config = builder.build(&parse_result.config);

    // Prepare node metrics (wrap/truncation + box height) before layout.
    let mut graph = parse_result.graph;
    measure::measure_graph(&mut graph, &config);

    // Run layout algorithm (may add warnings)
    let t_layout_start = std::time::Instant::now();
    let layout_config = if cli.compact {
        layout::CoarseLayoutConfig::compact()
    } else {
        layout::CoarseLayoutConfig::default()
    };
    let graph = layout::coarse_waterfall_with_config(graph, layout_config)?;
    if debug_timing {
        eprintln!("termiflow: layout {:?}", t_layout_start.elapsed());
        eprintln!(
            "termiflow: edge routes {}",
            graph
                .edge_routes
                .values()
                .filter(|r| !r.segments.is_empty())
                .count()
        );
        for (idx, e) in graph.edges.iter().enumerate() {
            eprintln!(
                "edge[{idx}] {} -> {} back_edge={}",
                e.from, e.to, e.is_back_edge
            );
        }
    }

    // Print any warnings to stderr (parser + layout)
    for warning in &graph.warnings {
        eprintln!("{}", warning);
    }

    if debug_routes {
        eprintln!("-- edge routes --");
        for (idx, e) in graph.edges.iter().enumerate() {
            if let Some(route) = graph.edge_routes.get(&idx) {
                eprint!("edge {} -> {}: ", e.from, e.to);
                for seg in &route.segments {
                    eprint!(
                        "({},{})→({},{}) ",
                        seg.from.x, seg.from.y, seg.to.x, seg.to.y
                    );
                }
                eprintln!();
            }
        }
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
            eprintln!("edge {} -> {} (back_edge={})", e.from, e.to, e.is_back_edge);
        }
    }

    // Render to canvas
    let t_render_start = std::time::Instant::now();
    let output = render_canvas(&graph, &config)?;
    if debug_timing {
        eprintln!("termiflow: render {:?}", t_render_start.elapsed());
    }

    // Print to stdout
    print!("{}", output);

    Ok(())
}

fn run_tui_mode(_cli: &Cli) -> Result<()> {
    // TODO: Implement TUI mode on Day 4
    eprintln!("termiflow: TUI mode not yet implemented. Use print mode for now.");
    std::process::exit(1);
}

fn read_input(cli: &Cli) -> Result<String> {
    use std::io::Read;

    if let Some(path) = cli.input_path() {
        return std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e));
    }

    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

impl Cli {
    /// Unified accessor for the input file (positional or -f/--file)
    fn input_path(&self) -> Option<&PathBuf> {
        // `--print [FILE]` supports both stdin (`--print`) and file input (`--print path.md`).
        // When `--print -` is used (or `--print` without value), fall back to stdin.
        if let Some(path) = self.print.as_ref() {
            if path.as_os_str() != "-" {
                return Some(path);
            }
        }
        self.file.as_ref().or(self.file_flag.as_ref())
    }
}
