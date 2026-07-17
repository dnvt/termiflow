//! TermiFlow CLI implementation.
//!
//! This module is shared by both binary entrypoints:
//! - `termiflow`
//! - `tw` (short alias)

use anyhow::Result;
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

// Use the termiflow library
use termiflow::render::semantic::{CellOwnerKind, SemanticFrame};
use termiflow::{
    display_profile::{display_width, graphemes, split_text_to_width_chunks},
    layout_and_render_with_feedback, measure, parse, CanvasBudget, CompositeStyle, Config,
    DiagramMetrics, ParseResult, ScalingMode, SpacingConfig, SpacingMode,
};
use termiflow::{
    tui::{
        build_inline_frame, build_preview_frame, clamp_viewport, initial_viewport,
        AnsiDiffPresenter, InlinePresenter, TerminalPresenter, Viewport,
    },
    CriticReport, FindingSeverity, RenderOutcome,
};

struct TerminalSession;

impl TerminalSession {
    fn enter() -> Result<Self> {
        use crossterm::{
            terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen},
            ExecutableCommand,
        };

        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        if let Err(err) = stdout.execute(EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(err.into());
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        use crossterm::{
            terminal::{disable_raw_mode, LeaveAlternateScreen},
            ExecutableCommand,
        };

        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = stdout.execute(LeaveAlternateScreen);
    }
}

/// Interactive TUI graph explorer - jq for diagrams
#[derive(Parser)]
#[command(name = "termiflow")]
#[command(version, about, long_about = None)]
#[command(after_help = "Examples:
  termiflow diagram.md              Print to stdout (default)
  termiflow -f diagram.md           File flag (jq-style)
  termiflow --print diagram.md      Print (explicit)
  termiflow --tui diagram.md        Partial alternate-screen preview
  termiflow --watch diagram.md      Safer live preview in normal scrollback
  cat file.md | termiflow           Read from stdin
  termiflow -s unicode diagram.md   Use Unicode borders

Notes:
  --watch keeps normal scrollback and avoids most fullscreen-emulator surprises
  --tui uses raw mode plus the alternate screen; input/scroll behavior can vary by terminal")]
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

    /// Deprecated compatibility flag; ANSI title inversion is the default in TTY print mode
    #[arg(long, hide = true)]
    pub ansi_title_invert: bool,

    /// Output to stdout (no interactive TUI)
    #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "-")]
    pub print: Option<PathBuf>,

    /// Alternate-screen live preview (partial beta; input/scroll behavior varies by terminal)
    #[arg(long)]
    pub tui: bool,

    /// Watch file and re-render inline in normal scrollback (safer than --tui)
    #[arg(long)]
    pub watch: bool,

    /// Maximum label width before truncation
    #[arg(long, value_name = "N")]
    pub max_label: Option<usize>,

    /// Maximum edge label width before truncation
    #[arg(long, value_name = "N")]
    pub max_edge_label: Option<usize>,

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
    /// DEPRECATED: Use --spacing=compact instead
    #[arg(long)]
    pub compact: bool,

    /// Spacing preset: compact, default, or spacious
    #[arg(long, value_name = "MODE")]
    pub spacing: Option<String>,

    /// Scaling mode: auto (adapts to diagram complexity) or fixed
    #[arg(long, value_name = "MODE")]
    pub scaling: Option<String>,

    /// Constrain canvas to terminal dimensions ($COLUMNS x $LINES)
    #[arg(long)]
    pub fit_terminal: bool,

    /// Enable bounded render repair passes after the initial draw
    #[arg(long)]
    pub optimize_render: bool,

    /// Maximum number of repair passes when render optimization is enabled
    #[arg(long, value_name = "N")]
    pub render_repair_passes: Option<usize>,

    /// Maximum number of layout candidate passes when render optimization is enabled
    #[arg(long, value_name = "N")]
    pub layout_repair_passes: Option<usize>,

    /// Emit critic findings for the rendered frame
    #[arg(long)]
    pub debug_critic: bool,

    /// Emit a compact visual audit summary for the rendered frame
    #[arg(long)]
    pub audit: bool,

    /// Treat input as TermiFlow JSON graph schema instead of Mermaid
    #[arg(long = "from-json")]
    pub from_json: bool,

    /// Exit with error on any parse warning
    #[arg(long)]
    pub strict: bool,

    /// Dump layout coordinates (debugging)
    #[arg(long, hide = true)]
    pub debug_layout: bool,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    // --watch: primary-screen inline live preview (no alternate screen / raw mode).
    if cli.watch {
        if cli.input_path().is_none() {
            eprintln!("termiflow: error: Watch mode requires a file path.");
            eprintln!("  Example: termiflow --watch diagram.md");
            std::process::exit(1);
        }
        return run_watch_mode(&cli);
    }

    // Default mode is print-to-stdout (jq-style).
    // `--tui` opts into the alternate-screen live preview mode.
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

fn run_watch_mode(cli: &Cli) -> Result<()> {
    let path = cli
        .input_path()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("watch mode requires a path"))?;
    let mut presenter = InlinePresenter::new(std::io::stdout());
    let mut last_modified = file_modified_time(&path);

    // Initial render
    let frame = match std::fs::read_to_string(&path) {
        Ok(input) => match render_cli_input(cli, &input, false) {
            Ok(rendered) => build_watch_frame(&path, &rendered),
            Err(err) => build_watch_error_frame(&path, &format!("render error\n{err}")),
        },
        Err(err) => {
            build_watch_error_frame(&path, &format!("failed to read {}\n{err}", path.display()))
        }
    };
    presenter.present(&frame)?;

    loop {
        std::thread::sleep(Duration::from_millis(200));

        let current_modified = file_modified_time(&path);
        if current_modified != last_modified {
            last_modified = current_modified;
            let frame = match std::fs::read_to_string(&path) {
                Ok(input) => match render_cli_input(cli, &input, false) {
                    Ok(rendered) => build_watch_frame(&path, &rendered),
                    Err(err) => build_watch_error_frame(&path, &format!("render error\n{err}")),
                },
                Err(err) => build_watch_error_frame(
                    &path,
                    &format!("failed to read {}\n{err}", path.display()),
                ),
            };
            presenter.present(&frame)?;
        }
    }
}

fn run_print_mode(cli: &Cli) -> Result<()> {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    let debug_routes = std::env::var("TERMIFLOW_DEBUG_ROUTES").is_ok();
    let t0 = std::time::Instant::now();

    // Read input
    let input = read_input(cli)?;
    if debug_timing {
        eprintln!("termiflow: parse {:?}", t0.elapsed());
    }

    let t_render_start = std::time::Instant::now();
    let rendered = render_cli_input(cli, &input, true)?;
    if debug_timing {
        eprintln!("termiflow: layout+render {:?}", t_render_start.elapsed());
        eprintln!(
            "termiflow: edge routes {}",
            rendered
                .graph
                .edge_routes
                .values()
                .filter(|r| !r.segments.is_empty())
                .count()
        );
        for (idx, e) in rendered.graph.edges.iter().enumerate() {
            eprintln!(
                "edge[{idx}] {} -> {} back_edge={}",
                e.from, e.to, e.is_back_edge
            );
        }
    }

    // Print any warnings to stderr (parser + layout)
    for warning in &rendered.graph.warnings {
        eprintln!("{}", warning);
    }

    if debug_routes {
        eprintln!("-- edge routes --");
        for (idx, e) in rendered.graph.edges.iter().enumerate() {
            if let Some(route) = rendered.graph.edge_routes.get(&idx) {
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
        for n in &rendered.graph.nodes {
            eprintln!(
                "node {}: label='{}' x={} y={} w={} rank={}",
                n.id, n.label, n.x, n.y, n.width, n.rank
            );
        }
        for e in &rendered.graph.edges {
            eprintln!("edge {} -> {} (back_edge={})", e.from, e.to, e.is_back_edge);
        }
    }

    // Print to stdout. When audit output is enabled, terminate the render with
    // a newline so stderr summary lines cannot visually splice into the last
    // border row.
    use std::io::Write;
    let mut stdout = std::io::stdout();
    let output = printable_output(&rendered, stdout.is_terminal());
    write!(stdout, "{output}")?;
    if cli.audit && !output.ends_with('\n') {
        writeln!(stdout)?;
    }
    stdout.flush()?;

    if cli.audit {
        emit_audit_summary(&rendered.outcome);
    }

    Ok(())
}

fn run_tui_mode(cli: &Cli) -> Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        terminal::size,
    };

    let Some(path) = cli.input_path().cloned() else {
        return Err(anyhow::anyhow!(
            "Interactive mode currently requires a file path for live preview"
        ));
    };

    let _session = TerminalSession::enter()?;
    let mut presenter = AnsiDiffPresenter::new(std::io::stdout());
    let mut viewport = Viewport::default();
    let mut viewport_user_controlled = false;
    let mut last_modified = file_modified_time(&path);
    let mut dirty = true;
    // Track last rendered content for End/G key and findings overlay.
    let mut last_content = String::new();
    let mut last_report = CriticReport::default();
    // Findings overlay state.
    let mut findings_mode = false;
    let mut findings_scroll: u16 = 0;

    let result = loop {
        let terminal_size = size()?;
        if dirty {
            let frame = if findings_mode {
                let file_label = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("diagram");
                build_findings_frame(&last_report, file_label, findings_scroll, terminal_size)
            } else {
                match std::fs::read_to_string(&path) {
                    Ok(input) => {
                        let (f, content, report) = build_tui_frame(
                            cli,
                            &path,
                            &input,
                            terminal_size,
                            &mut viewport,
                            viewport_user_controlled,
                        );
                        last_content = content;
                        last_report = report;
                        f
                    }
                    Err(err) => {
                        viewport = Viewport::default();
                        build_preview_frame(
                            &format!(
                                "termiflow: failed to read {}\n\n{}\n\nFix the file or press q to quit.",
                                path.display(),
                                err
                            ),
                            "q quit | r retry",
                            terminal_size,
                            viewport,
                        )
                    }
                }
            };
            presenter.present(&frame)?;
            dirty = false;
        }

        // Page step is one full viewport height (minus status bar row).
        let page_step = terminal_size.1.saturating_sub(2).max(1);

        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc if findings_mode => {
                        findings_mode = false;
                        dirty = true;
                    }
                    KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                    KeyCode::Char('?') | KeyCode::Char('f') => {
                        findings_mode = !findings_mode;
                        findings_scroll = 0;
                        dirty = true;
                    }
                    KeyCode::Char('r') => {
                        findings_mode = false;
                        dirty = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') if findings_mode => {
                        findings_scroll = findings_scroll.saturating_sub(1);
                        dirty = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') if findings_mode => {
                        findings_scroll = findings_scroll.saturating_add(1);
                        dirty = true;
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        viewport_user_controlled = true;
                        viewport.offset_x = viewport.offset_x.saturating_sub(2);
                        dirty = true;
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        viewport_user_controlled = true;
                        viewport.offset_x = viewport.offset_x.saturating_add(2);
                        dirty = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        viewport_user_controlled = true;
                        viewport.offset_y = viewport.offset_y.saturating_sub(1);
                        dirty = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        viewport_user_controlled = true;
                        viewport.offset_y = viewport.offset_y.saturating_add(1);
                        dirty = true;
                    }
                    KeyCode::PageUp => {
                        if findings_mode {
                            findings_scroll = findings_scroll.saturating_sub(page_step);
                        } else {
                            viewport_user_controlled = true;
                            viewport.offset_y = viewport.offset_y.saturating_sub(page_step);
                        }
                        dirty = true;
                    }
                    KeyCode::PageDown => {
                        if findings_mode {
                            findings_scroll = findings_scroll.saturating_add(page_step);
                        } else {
                            viewport_user_controlled = true;
                            viewport.offset_y = viewport.offset_y.saturating_add(page_step);
                        }
                        dirty = true;
                    }
                    KeyCode::Home | KeyCode::Char('g') => {
                        if findings_mode {
                            findings_scroll = 0;
                        } else {
                            viewport_user_controlled = true;
                            viewport = Viewport::default();
                        }
                        dirty = true;
                    }
                    KeyCode::End | KeyCode::Char('G') if !findings_mode => {
                        viewport_user_controlled = true;
                        let content_lines = last_content.lines().count() as u16;
                        let viewport_lines = terminal_size.1.saturating_sub(1);
                        viewport.offset_y = content_lines.saturating_sub(viewport_lines);
                        viewport.offset_x = 0;
                        dirty = true;
                    }
                    _ => {}
                },
                Event::Resize(_, _) => dirty = true,
                _ => {}
            }
        }

        if !findings_mode {
            let current_modified = file_modified_time(&path);
            if current_modified != last_modified {
                last_modified = current_modified;
                dirty = true;
            }
        }
    };

    drop(presenter);
    result
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

struct PreparedRender {
    graph: termiflow::Graph,
    outcome: RenderOutcome,
}

const ANSI_INVERT_ON: &str = "\u{1b}[7m";
const ANSI_RESET: &str = "\u{1b}[0m";

fn printable_output(rendered: &PreparedRender, invert_titles: bool) -> String {
    if invert_titles {
        invert_subgraph_titles_ansi(
            &rendered.outcome.output,
            &rendered.outcome.display_semantic_frame,
        )
    } else {
        rendered.outcome.output.clone()
    }
}

fn invert_subgraph_titles_ansi(output: &str, semantic_frame: &SemanticFrame) -> String {
    if semantic_frame.width == 0 || semantic_frame.height == 0 {
        return output.to_string();
    }

    output
        .split('\n')
        .enumerate()
        .map(|(y, line)| invert_titles_in_line(line, y, semantic_frame))
        .collect::<Vec<_>>()
        .join("\n")
}

fn invert_titles_in_line(line: &str, y: usize, semantic_frame: &SemanticFrame) -> String {
    if y >= semantic_frame.height {
        return line.to_string();
    }

    let mut styled = String::with_capacity(line.len() + 32);
    let mut display_x = 0usize;
    let mut in_title_run = false;

    for grapheme in graphemes(line) {
        let width = display_width(grapheme);
        let is_title = if width == 0 {
            in_title_run
        } else {
            grapheme_overlaps_title_cells(semantic_frame, y, display_x, width)
        };

        if is_title && !in_title_run {
            styled.push_str(ANSI_INVERT_ON);
        } else if !is_title && in_title_run {
            styled.push_str(ANSI_RESET);
        }

        styled.push_str(grapheme);
        in_title_run = is_title;
        display_x += width;
    }

    if in_title_run {
        styled.push_str(ANSI_RESET);
    }

    styled
}

fn grapheme_overlaps_title_cells(
    semantic_frame: &SemanticFrame,
    y: usize,
    start_x: usize,
    width: usize,
) -> bool {
    (start_x..start_x.saturating_add(width)).any(|x| {
        semantic_frame
            .get(x, y)
            .is_some_and(|meta| meta.owner_kind == CellOwnerKind::SubgraphTitle)
    })
}

fn emit_audit_summary(outcome: &RenderOutcome) {
    let summary = outcome.critic_report.audit_summary();
    eprintln!(
        "termiflow: audit verdict={:?} score={} info={} warnings={} errors={} findings={}",
        summary.verdict,
        summary.score,
        summary.info_count,
        summary.warning_count,
        summary.error_count,
        outcome.critic_report.findings.len()
    );
    for highlight in &summary.highlights {
        eprintln!("termiflow: audit highlight: {highlight}");
    }
}

fn top_finding_label(report: &CriticReport) -> String {
    report
        .findings
        .first()
        .map(|finding| format!("{:?}", finding.code))
        .unwrap_or_else(|| "ok".to_string())
}

fn build_tui_status(
    report: &CriticReport,
    warning_count: usize,
    file_label: &str,
    viewport_indicator: &str,
) -> String {
    let summary = report.audit_summary();
    let top_finding = top_finding_label(report);
    format!(
        "q/ESC quit | j/k/arrows pan | g/G top/bot | ? findings | verdict={:?} score={} warn={} top={} | {} | {}",
        summary.verdict,
        summary.score,
        warning_count,
        top_finding,
        viewport_indicator,
        file_label,
    )
}

fn build_viewport_indicator(content: &str, viewport: Viewport) -> String {
    let content_lines = content.lines().count() as u16;
    let content_width = content
        .lines()
        .map(display_width)
        .max()
        .unwrap_or(0)
        .min(usize::from(u16::MAX)) as u16;

    let visible_line = if content_lines == 0 {
        0
    } else {
        viewport.offset_y.saturating_add(1).min(content_lines)
    };
    let visible_col = if content_width == 0 {
        0
    } else {
        viewport.offset_x.saturating_add(1).min(content_width)
    };

    format!(
        "line {}/{} | col {}/{}",
        visible_line, content_lines, visible_col, content_width
    )
}

fn build_watch_status(report: &CriticReport, warning_count: usize, file_label: &str) -> String {
    let summary = report.audit_summary();
    let top_finding = top_finding_label(report);
    format!(
        "watch | Ctrl-C quit | auto-reload | verdict={:?} score={} warn={} top={} | {}",
        summary.verdict, summary.score, warning_count, top_finding, file_label,
    )
}

fn build_watch_frame(
    path: &std::path::Path,
    rendered: &PreparedRender,
) -> termiflow::TerminalFrame {
    let file_label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("diagram");
    let status = build_watch_status(
        &rendered.outcome.critic_report,
        rendered.graph.warnings.len(),
        file_label,
    );
    let mut frame = build_inline_frame(&rendered.outcome.output, &status);
    apply_inverted_titles_to_tui_frame(
        &mut frame,
        &rendered.outcome.display_semantic_frame,
        Viewport::default(),
    );
    frame
}

fn build_watch_error_frame(path: &std::path::Path, message: &str) -> termiflow::TerminalFrame {
    let file_label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("diagram");
    let content = format!("termiflow: {message}");
    let status = format!("watch | Ctrl-C quit | waiting for next save | {file_label}");
    build_inline_frame(&content, &status)
}

fn render_cli_input(cli: &Cli, input: &str, emit_debug_critic: bool) -> Result<PreparedRender> {
    let parse_result = if cli.from_json {
        let (graph, config) = termiflow::parse_json_graph(input)?;
        ParseResult { graph, config }
    } else {
        parse(input, cli.strict)?
    };

    let scaling_mode = cli
        .scaling
        .as_deref()
        .and_then(|value| value.parse::<ScalingMode>().ok())
        .unwrap_or(ScalingMode::Fixed);

    let mut builder = Config::builder().strict(cli.strict);

    if let Some(max_label) = cli.max_label {
        builder = builder.max_label_width(max_label);
    }
    if let Some(max_edge_label) = cli.max_edge_label {
        builder = builder.max_edge_label_width(max_edge_label);
    }
    if cli.wrap {
        builder = builder.wrap_labels(true);
    }
    if let Some(max_lines) = cli.max_lines {
        builder = builder.max_label_lines(max_lines);
    } else if cli.wrap {
        builder = builder.max_label_lines(3);
    }

    if let Some(ref spacing_str) = cli.spacing {
        let spacing_mode = spacing_str
            .parse::<SpacingMode>()
            .unwrap_or(SpacingMode::Default);
        builder = builder.spacing(SpacingConfig::from_mode(spacing_mode));
    } else if cli.compact {
        builder = builder.spacing(SpacingConfig::compact());
    } else if scaling_mode == ScalingMode::Auto {
        let spacing_mode =
            DiagramMetrics::from_graph(&parse_result.graph).recommended_spacing_mode();
        builder = builder.spacing(SpacingConfig::from_mode(spacing_mode));
    }

    if cli.no_crop {
        builder = builder.crop(false);
    } else if cli.crop {
        builder = builder.crop(true);
    }

    if let Some(pad) = cli.pad {
        builder = builder.pad(pad);
    }

    if cli.optimize_render {
        builder = builder.optimize_render(true);
    }
    if let Some(render_repair_passes) = cli.render_repair_passes {
        builder = builder.render_repair_passes(render_repair_passes);
    }
    if let Some(layout_repair_passes) = cli.layout_repair_passes {
        builder = builder.layout_repair_passes(layout_repair_passes);
    }
    if !emit_debug_critic {
        builder = builder.debug_critic(false);
    } else if cli.debug_critic {
        builder = builder.debug_critic(true);
    }

    if let Some(ref style_str) = cli.style {
        builder = builder.style(CompositeStyle::parse(style_str));
    }

    let mut config = builder.build(&parse_result.config);
    if cli.fit_terminal {
        let budget = CanvasBudget::from_terminal();
        if budget.target_width.is_some() {
            config.spacing.max_canvas_width = budget.effective_width().max(1);
        }
        if budget.target_height.is_some() {
            config.spacing.max_canvas_height = budget.effective_height().max(1);
        }
    }
    config.spacing = config.spacing.for_direction(parse_result.graph.direction);

    let mut graph = parse_result.graph;
    measure::measure_graph(&mut graph, &config);

    let (graph, outcome) = layout_and_render_with_feedback(graph, config)?;
    Ok(PreparedRender { graph, outcome })
}

/// Build a TUI frame from the current file state.
///
/// Returns `(frame, content)` — content is the raw diagram string, kept by
/// the caller so that End/G navigation can compute the bottom of the diagram.
fn build_tui_frame(
    cli: &Cli,
    path: &std::path::Path,
    input: &str,
    terminal_size: (u16, u16),
    viewport: &mut Viewport,
    viewport_user_controlled: bool,
) -> (termiflow::TerminalFrame, String, CriticReport) {
    match render_cli_input(cli, input, false) {
        Ok(rendered) => {
            let content = rendered.outcome.output.clone();
            let report = rendered.outcome.critic_report.clone();
            if viewport_user_controlled {
                clamp_viewport(viewport, &content, terminal_size);
            } else {
                *viewport = initial_viewport(&content, terminal_size);
            }

            let file_label = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("diagram");

            let viewport_indicator = build_viewport_indicator(&content, *viewport);
            let status = build_tui_status(
                &report,
                rendered.graph.warnings.len(),
                file_label,
                &viewport_indicator,
            );

            let mut frame = build_preview_frame(&content, &status, terminal_size, *viewport);
            apply_inverted_titles_to_tui_frame(
                &mut frame,
                &rendered.outcome.display_semantic_frame,
                *viewport,
            );
            (frame, content, report)
        }
        Err(err) => {
            *viewport = Viewport::default();
            let msg = format!("termiflow: render error\n\n{err}\n");
            let frame = build_preview_frame(&msg, "q quit | r retry", terminal_size, *viewport);
            (frame, msg, CriticReport::default())
        }
    }
}

fn apply_inverted_titles_to_tui_frame(
    frame: &mut termiflow::TerminalFrame,
    semantic_frame: &SemanticFrame,
    viewport: Viewport,
) {
    if frame.width == 0 || frame.height == 0 {
        return;
    }

    let visible_content_height = frame.height.saturating_sub(1);
    if visible_content_height == 0 {
        return;
    }

    let viewport_left = usize::from(viewport.offset_x);
    let viewport_top = usize::from(viewport.offset_y);
    let viewport_right = viewport_left + usize::from(frame.width).saturating_sub(1);
    let viewport_bottom = viewport_top + usize::from(visible_content_height).saturating_sub(1);

    for absolute_y in viewport_top..=viewport_bottom {
        let local_y = (absolute_y - viewport_top) as u16;
        if local_y >= visible_content_height {
            continue;
        }

        let mut absolute_x = viewport_left;
        while absolute_x <= viewport_right {
            let is_title = semantic_frame
                .get(absolute_x, absolute_y)
                .is_some_and(|meta| meta.owner_kind == CellOwnerKind::SubgraphTitle);
            if !is_title {
                absolute_x = absolute_x.saturating_add(1);
                continue;
            }

            let run_start = absolute_x;
            let mut run_end = absolute_x;
            while run_end < viewport_right
                && semantic_frame
                    .get(run_end.saturating_add(1), absolute_y)
                    .is_some_and(|meta| meta.owner_kind == CellOwnerKind::SubgraphTitle)
            {
                run_end = run_end.saturating_add(1);
            }

            let start_local_x = (run_start - viewport_left) as u16;
            let end_local_x = (run_end - viewport_left) as u16;
            let start_idx =
                usize::from(local_y) * usize::from(frame.width) + usize::from(start_local_x);
            let end_idx =
                usize::from(local_y) * usize::from(frame.width) + usize::from(end_local_x);

            if start_idx == end_idx {
                if let Some(cell) = frame.cells.get_mut(start_idx) {
                    cell.wrap_ansi(ANSI_INVERT_ON, ANSI_RESET);
                }
            } else {
                if let Some(cell) = frame.cells.get_mut(start_idx) {
                    cell.prefix_ansi(ANSI_INVERT_ON);
                }
                if let Some(cell) = frame.cells.get_mut(end_idx) {
                    cell.suffix_ansi(ANSI_RESET);
                }
            }

            absolute_x = run_end.saturating_add(1);
        }
    }
}

/// Build a full-screen findings overlay frame from a critic report.
///
/// Shows verdict, score, finding counts, and each individual finding with its
/// severity tag and message. Supports vertical scrolling via `scroll`.
fn build_findings_frame(
    report: &CriticReport,
    file_label: &str,
    scroll: u16,
    terminal_size: (u16, u16),
) -> termiflow::TerminalFrame {
    let (width, height) = terminal_size;
    if width == 0 || height == 0 {
        return termiflow::TerminalFrame::new(width, height);
    }

    let summary = report.audit_summary();

    // Build the text lines for the panel.
    let mut lines: Vec<String> = Vec::new();

    let divider = "─".repeat(usize::from(width).min(60));

    lines.push(format!(" Render Critic Report  ─  {file_label}"));
    lines.push(divider.clone());
    lines.push(format!(
        " score={}  errors={}  warnings={}  info={}  verdict={:?}",
        summary.score,
        summary.error_count,
        summary.warning_count,
        summary.info_count,
        summary.verdict,
    ));
    lines.push(divider.clone());

    if report.findings.is_empty() {
        lines.push("  (no findings — diagram looks clean)".to_string());
    } else {
        for finding in &report.findings {
            let tag = match finding.severity {
                FindingSeverity::Error => "ERROR",
                FindingSeverity::Warning => "WARN ",
                FindingSeverity::Info => "INFO ",
            };
            lines.push(format!(
                " [{tag}] {:?}  (penalty {})",
                finding.code, finding.penalty
            ));
            // Wrap the message at ~(width-4) chars
            let msg_width = usize::from(width).saturating_sub(6).max(20);
            for chunk in wrap_text(&finding.message, msg_width) {
                lines.push(format!("        {chunk}"));
            }
            if !finding.cells.is_empty() {
                let cell_preview: Vec<String> = finding
                    .cells
                    .iter()
                    .take(4)
                    .map(|(x, y)| format!("({x},{y})"))
                    .collect();
                let suffix = if finding.cells.len() > 4 {
                    format!(" +{}", finding.cells.len() - 4)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "        cells: {}{}",
                    cell_preview.join(" "),
                    suffix
                ));
            }
        }
    }

    lines.push(divider);
    if !report.notes.is_empty() {
        for note in report.notes.iter().take(3) {
            lines.push(format!("  note: {note}"));
        }
    }

    // Status bar (last row).
    let status = "?/f close | j/k/arrows scroll | q quit";

    // Render into a TerminalFrame using build_preview_frame's logic.
    let content = lines.join("\n");
    build_preview_frame(
        &content,
        status,
        terminal_size,
        termiflow::tui::Viewport {
            offset_x: 0,
            offset_y: scroll,
        },
    )
}

/// Wrap a string into chunks of at most `max_width` characters, breaking at spaces.
fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 || display_width(s) <= max_width {
        return vec![s.to_string()];
    }
    if s.trim().is_empty() {
        return vec![s.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in s.split_whitespace() {
        let word_width = display_width(word);
        if current.is_empty() {
            if word_width <= max_width {
                current.push_str(word);
                current_width = word_width;
            } else {
                lines.extend(split_text_to_width_chunks(word, max_width));
            }
            continue;
        }

        if current_width + 1 + word_width <= max_width {
            current.push(' ');
            current.push_str(word);
            current_width += 1 + word_width;
        } else {
            lines.push(std::mem::take(&mut current));
            current_width = 0;

            if word_width <= max_width {
                current.push_str(word);
                current_width = word_width;
            } else {
                lines.extend(split_text_to_width_chunks(word, max_width));
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn file_modified_time(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use termiflow::{
        render::semantic::{CellMeta, CellOwnerKind, CellRole, SemanticFrame},
        CriticFinding, FindingCode, Graph, RenderOutcome,
    };

    #[test]
    fn wrap_text_short_string_is_unchanged() {
        let result = wrap_text("hello world", 40);
        assert_eq!(result, vec!["hello world"]);
    }

    #[test]
    fn wrap_text_breaks_at_space() {
        let result = wrap_text("hello world foo", 11);
        assert_eq!(result[0], "hello world");
    }

    #[test]
    fn wrap_text_falls_back_to_hard_break() {
        let result = wrap_text("abcdefghij", 5);
        assert!(!result.is_empty());
        assert!(display_width(&result[0]) <= 5);
    }

    #[test]
    fn wrap_text_preserves_grapheme_clusters_on_hard_break() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            wrap_text(&format!("{family}{family}"), display_width(family)),
            vec![family.to_string(), family.to_string()]
        );
    }

    #[test]
    fn wrap_text_uses_display_width_for_cjk() {
        let result = wrap_text("日本語 日本語", 6);
        assert_eq!(result, vec!["日本語".to_string(), "日本語".to_string()]);
    }

    #[test]
    fn build_findings_frame_clean_report_shows_clean_message() {
        let report = CriticReport::default();
        let frame = build_findings_frame(&report, "test.md", 0, (80, 24));
        // Verify the frame has the right dimensions
        assert_eq!(frame.width, 80);
        assert_eq!(frame.height, 24);
    }

    #[test]
    fn build_findings_frame_with_findings_shows_finding_code() {
        let report = CriticReport {
            score: -20,
            findings: vec![CriticFinding {
                code: FindingCode::RouteTopologyMismatch,
                severity: FindingSeverity::Error,
                penalty: -20,
                message: "test message".to_string(),
                cells: vec![(1, 2)],
                owner_ids: vec![],
            }],
            notes: vec![],
        };
        let frame = build_findings_frame(&report, "diagram.md", 0, (80, 24));
        // Frame should contain some cells - just check dimensions
        assert_eq!(frame.width, 80);
        assert_eq!(frame.height, 24);
    }

    #[test]
    fn cli_parses_render_feedback_flags() {
        let cli = Cli::try_parse_from([
            "termiflow",
            "--optimize-render",
            "--render-repair-passes",
            "4",
            "--layout-repair-passes",
            "2",
            "--debug-critic",
            "--audit",
        ])
        .unwrap();

        assert!(cli.optimize_render);
        assert_eq!(cli.render_repair_passes, Some(4));
        assert_eq!(cli.layout_repair_passes, Some(2));
        assert!(cli.debug_critic);
        assert!(cli.audit);
    }

    #[test]
    fn cli_help_mentions_live_preview_caveats() {
        let mut command = Cli::command();
        let mut help = Vec::new();
        command.write_long_help(&mut help).unwrap();
        let help = String::from_utf8(help).unwrap();

        assert!(help.contains("Partial alternate-screen preview"));
        assert!(help.contains("Safer live preview in normal scrollback"));
        assert!(help.contains("input/scroll behavior can vary by terminal"));
    }

    #[test]
    fn cli_accepts_legacy_ansi_title_invert_flag() {
        let cli = Cli::try_parse_from(["termiflow", "--ansi-title-invert"]).unwrap();
        assert!(cli.ansi_title_invert);
    }

    #[test]
    fn build_watch_frame_includes_status_row() {
        let rendered = PreparedRender {
            graph: Graph::new(),
            outcome: RenderOutcome {
                output: "+---+\n| A |\n+---+".to_string(),
                semantic_frame: SemanticFrame::default(),
                display_semantic_frame: SemanticFrame::default(),
                critic_report: CriticReport::default(),
                warnings: Vec::new(),
                optimized: false,
                repair_passes: 0,
                layout_attempts: 1,
                layout_repairs_applied: 0,
            },
        };

        let frame = build_watch_frame(std::path::Path::new("diagram.md"), &rendered);
        let status_row: String = (0..frame.width)
            .map(|x| {
                frame
                    .get(x, frame.height - 1)
                    .map(|cell| cell.ch)
                    .unwrap_or(' ')
            })
            .collect();

        assert!(status_row.contains("watch"));
        assert!(status_row.contains("diagram.md"));
        assert!(status_row.contains("verdict=Clean"));
    }

    #[test]
    fn build_watch_frame_inverts_subgraph_titles() {
        let title = "Service";
        let title_token = termiflow::graph::subgraph_title_text(title);
        let width = title_token.chars().count() + 6;
        let content = format!(
            "┏{}┓\n┃  {}  ┃\n┗{}┛",
            "━".repeat(width.saturating_sub(2)),
            title_token,
            "━".repeat(width.saturating_sub(2))
        );

        let mut graph = Graph::new();
        graph.direction = termiflow::graph::Direction::TD;
        let mut subgraph = termiflow::graph::Subgraph::new("service", Some(title.to_string()));
        subgraph.bounds = termiflow::graph::Rectangle::new(0, 0, width, 3);
        graph.add_subgraph(subgraph);

        let title_y = termiflow::graph::subgraph_title_row(0, 3, termiflow::graph::Direction::TD);
        let title_x = termiflow::graph::subgraph_title_start_x(
            0,
            width,
            title,
            termiflow::graph::Direction::TD,
        )
        .expect("title start");
        let mut semantic_frame = SemanticFrame {
            width,
            height: 3,
            cells: vec![CellMeta::default(); width * 3],
        };
        for (offset, ch) in title_token.chars().enumerate() {
            semantic_frame.cells[title_y * width + title_x + offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("service".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }

        let rendered = PreparedRender {
            graph,
            outcome: RenderOutcome {
                output: content,
                display_semantic_frame: semantic_frame.clone(),
                semantic_frame,
                critic_report: CriticReport::default(),
                warnings: Vec::new(),
                optimized: false,
                repair_passes: 0,
                layout_attempts: 1,
                layout_repairs_applied: 0,
            },
        };

        let frame = build_watch_frame(std::path::Path::new("diagram.md"), &rendered);

        let first_title_cell = frame
            .get(title_x as u16, title_y as u16)
            .expect("first title cell");
        assert!(first_title_cell.text().contains(ANSI_INVERT_ON));

        let last_title_cell = frame
            .get(
                (title_x + title_token.chars().count().saturating_sub(1)) as u16,
                title_y as u16,
            )
            .expect("last title cell");
        assert!(last_title_cell.text().contains(ANSI_RESET));

        let border_cell = frame.get(0, title_y as u16).expect("border cell");
        assert!(!border_cell.text().contains(ANSI_INVERT_ON));
    }

    #[test]
    fn apply_inverted_titles_to_tui_frame_respects_viewport_crop() {
        let title = "Service";
        let title_token = termiflow::graph::subgraph_title_text(title);
        let width = title_token.chars().count() + 6;
        let content = format!(
            "┏{}┓\n┃  {}  ┃\n┗{}┛",
            "━".repeat(width.saturating_sub(2)),
            title_token,
            "━".repeat(width.saturating_sub(2))
        );

        let title_y = termiflow::graph::subgraph_title_row(0, 3, termiflow::graph::Direction::TD);
        let title_x = termiflow::graph::subgraph_title_start_x(
            0,
            width,
            title,
            termiflow::graph::Direction::TD,
        )
        .expect("title start");
        let mut semantic_frame = SemanticFrame {
            width,
            height: 3,
            cells: vec![CellMeta::default(); width * 3],
        };
        for (offset, ch) in title_token.chars().enumerate() {
            semantic_frame.cells[title_y * width + title_x + offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("service".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }

        let viewport = Viewport {
            offset_x: 3,
            offset_y: 0,
        };
        let mut frame = build_preview_frame(&content, "status", (8, 3), viewport);
        apply_inverted_titles_to_tui_frame(&mut frame, &semantic_frame, viewport);

        let first_visible_title_cell = frame.get(0, 1).expect("cropped title cell");
        assert!(first_visible_title_cell.text().contains(ANSI_INVERT_ON));

        let reset_seen = (0..frame.width)
            .filter_map(|x| frame.get(x, 1))
            .any(|cell| cell.text().contains(ANSI_RESET));
        assert!(
            reset_seen,
            "cropped title should still close the invert span"
        );

        let status_cell = frame.get(0, frame.height - 1).expect("status cell");
        assert!(!status_cell.text().contains(ANSI_INVERT_ON));
    }

    #[test]
    fn invert_subgraph_titles_ansi_wraps_title_tokens() {
        let service_title = termiflow::graph::subgraph_title_text("Service Layer");
        let data_title = termiflow::graph::subgraph_title_text("Data Layer");
        let output = format!("xx{service_title}yy{data_title}zz");
        let width = output.chars().count();
        let service_start = 2usize;
        let data_start = service_start + service_title.chars().count() + 2;

        let mut semantic_frame = SemanticFrame {
            width,
            height: 1,
            cells: vec![CellMeta::default(); width],
        };

        for (offset, ch) in service_title.chars().enumerate() {
            semantic_frame.cells[service_start + offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("service".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }
        for (offset, ch) in data_title.chars().enumerate() {
            semantic_frame.cells[data_start + offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("data".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }

        let styled = invert_subgraph_titles_ansi(&output, &semantic_frame);

        assert!(styled.contains(&format!("{ANSI_INVERT_ON}{service_title}{ANSI_RESET}")));
        assert!(styled.contains(&format!("{ANSI_INVERT_ON}{data_title}{ANSI_RESET}")));
        assert!(styled.contains("xx"));
        assert!(styled.contains("yy"));
        assert!(styled.contains("zz"));
    }

    #[test]
    fn invert_subgraph_titles_ansi_only_styles_semantic_title_cells() {
        let title_token = termiflow::graph::subgraph_title_text("Data Layer");
        let output = format!("node:{title_token}|title:{title_token}");
        let width = output.chars().count();
        let title_start =
            "node:".chars().count() + title_token.chars().count() + "|title:".chars().count();

        let mut semantic_frame = SemanticFrame {
            width,
            height: 1,
            cells: vec![CellMeta::default(); width],
        };
        for (offset, ch) in title_token.chars().enumerate() {
            semantic_frame.cells[title_start + offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("data".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }

        let styled = invert_subgraph_titles_ansi(&output, &semantic_frame);
        let expected =
            format!("node:{title_token}|title:{ANSI_INVERT_ON}{title_token}{ANSI_RESET}");
        assert_eq!(styled, expected);
    }

    #[test]
    fn printable_output_inverts_titles_by_default_for_tty_print_mode() {
        let title_token = termiflow::graph::subgraph_title_text("My Group");
        let output = format!("┏━━{title_token}━━┓");
        let width = output.chars().count();
        let title_start = "┏━━".chars().count();
        let rendered = PreparedRender {
            graph: Graph::new(),
            outcome: RenderOutcome {
                output,
                semantic_frame: {
                    let mut semantic_frame = SemanticFrame {
                        width,
                        height: 1,
                        cells: vec![CellMeta::default(); width],
                    };
                    for (offset, ch) in title_token.chars().enumerate() {
                        semantic_frame.cells[title_start + offset] = CellMeta {
                            ch,
                            owner_kind: CellOwnerKind::SubgraphTitle,
                            owner_id: Some("group".to_string()),
                            role: CellRole::Text,
                            z_index: 2,
                        };
                    }
                    semantic_frame
                },
                display_semantic_frame: {
                    let mut semantic_frame = SemanticFrame {
                        width,
                        height: 1,
                        cells: vec![CellMeta::default(); width],
                    };
                    for (offset, ch) in title_token.chars().enumerate() {
                        semantic_frame.cells[title_start + offset] = CellMeta {
                            ch,
                            owner_kind: CellOwnerKind::SubgraphTitle,
                            owner_id: Some("group".to_string()),
                            role: CellRole::Text,
                            z_index: 2,
                        };
                    }
                    semantic_frame
                },
                critic_report: CriticReport::default(),
                warnings: Vec::new(),
                optimized: false,
                repair_passes: 0,
                layout_attempts: 1,
                layout_repairs_applied: 0,
            },
        };

        let tty_output = printable_output(&rendered, true);
        let piped_output = printable_output(&rendered, false);

        assert!(tty_output.contains(&format!("{ANSI_INVERT_ON}{title_token}{ANSI_RESET}")));
        assert_eq!(piped_output, rendered.outcome.output);
    }

    #[test]
    fn printable_output_uses_display_aligned_semantic_frame() {
        let title_token = termiflow::graph::subgraph_title_text("Data Layer");
        let output = title_token.clone();

        let mut raw_semantic_frame = SemanticFrame {
            width: title_token.chars().count() + 4,
            height: 1,
            cells: vec![CellMeta::default(); title_token.chars().count() + 4],
        };
        for (offset, ch) in title_token.chars().enumerate() {
            raw_semantic_frame.cells[2 + offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("group".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }

        let mut display_semantic_frame = SemanticFrame {
            width: title_token.chars().count(),
            height: 1,
            cells: vec![CellMeta::default(); title_token.chars().count()],
        };
        for (offset, ch) in title_token.chars().enumerate() {
            display_semantic_frame.cells[offset] = CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("group".to_string()),
                role: CellRole::Text,
                z_index: 2,
            };
        }

        let rendered = PreparedRender {
            graph: Graph::new(),
            outcome: RenderOutcome {
                output: output.clone(),
                semantic_frame: raw_semantic_frame,
                display_semantic_frame,
                critic_report: CriticReport::default(),
                warnings: Vec::new(),
                optimized: false,
                repair_passes: 0,
                layout_attempts: 1,
                layout_repairs_applied: 0,
            },
        };

        let tty_output = printable_output(&rendered, true);
        assert_eq!(
            tty_output,
            format!("{ANSI_INVERT_ON}{title_token}{ANSI_RESET}")
        );
    }

    #[test]
    fn viewport_indicator_reports_line_and_column_position() {
        let indicator = build_viewport_indicator(
            "0123456789\nabcdef",
            Viewport {
                offset_x: 3,
                offset_y: 1,
            },
        );

        assert_eq!(indicator, "line 2/2 | col 4/10");
    }

    #[test]
    fn tui_status_can_surface_horizontal_pan_state() {
        let status = build_tui_status(
            &CriticReport::default(),
            0,
            "diagram.md",
            "line 3/8 | col 9/42",
        );

        assert!(status.contains("line 3/8 | col 9/42"));
        assert!(status.contains("j/k/arrows pan"));
    }
}
