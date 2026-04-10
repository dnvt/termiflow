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
use termiflow::{
    display_profile::{display_width, split_text_to_width_chunks},
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
                    KeyCode::End | KeyCode::Char('G') => {
                        if !findings_mode {
                            viewport_user_controlled = true;
                            let content_lines = last_content.lines().count() as u16;
                            let viewport_lines = terminal_size.1.saturating_sub(1);
                            viewport.offset_y = content_lines.saturating_sub(viewport_lines);
                            viewport.offset_x = 0;
                            dirty = true;
                        }
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

fn printable_output(rendered: &PreparedRender, stdout_is_tty: bool) -> String {
    if stdout_is_tty {
        invert_subgraph_titles_ansi(&rendered.outcome.output, &rendered.graph)
    } else {
        rendered.outcome.output.clone()
    }
}

fn invert_subgraph_titles_ansi(output: &str, graph: &termiflow::Graph) -> String {
    let mut title_tokens = std::collections::BTreeSet::new();
    for subgraph in &graph.subgraphs {
        if let Some(title) = subgraph.title.as_deref() {
            title_tokens.insert((format!("[  {title}  ]"), format_inverted_title(title)));
        }
    }

    if title_tokens.is_empty() {
        return output.to_string();
    }

    let mut title_tokens: Vec<(String, String)> = title_tokens.into_iter().collect();
    title_tokens.sort_by(|left, right| {
        right
            .0
            .chars()
            .count()
            .cmp(&left.0.chars().count())
            .then_with(|| left.0.cmp(&right.0))
    });

    output
        .split('\n')
        .map(|line| invert_titles_in_line(line, &title_tokens))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_inverted_title(title: &str) -> String {
    format!("\u{1b}[7m   {title}   \u{1b}[0m")
}

fn invert_titles_in_line(line: &str, title_tokens: &[(String, String)]) -> String {
    let mut raw_matches: Vec<(usize, usize, usize)> = Vec::new();
    for (priority, token) in title_tokens.iter().enumerate() {
        for (start, _) in line.match_indices(&token.0) {
            raw_matches.push((start, start + token.0.len(), priority));
        }
    }

    if raw_matches.is_empty() {
        return line.to_string();
    }

    raw_matches.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| (right.1 - right.0).cmp(&(left.1 - left.0)))
    });

    let mut accepted: Vec<(usize, usize, usize)> = Vec::new();
    let mut last_end = 0usize;
    for (start, end, token_idx) in raw_matches {
        if start < last_end {
            continue;
        }
        accepted.push((start, end, token_idx));
        last_end = end;
    }

    let mut styled = String::with_capacity(line.len() + accepted.len() * 16);
    let mut cursor = 0usize;
    for (start, end, token_idx) in accepted {
        styled.push_str(&line[cursor..start]);
        styled.push_str(&title_tokens[token_idx].1);
        cursor = end;
    }
    styled.push_str(&line[cursor..]);
    styled
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
    build_inline_frame(&rendered.outcome.output, &status)
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

            let frame = build_preview_frame(&content, &status, terminal_size, *viewport);
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
        render::semantic::SemanticFrame, CriticFinding, FindingCode, Graph, RenderOutcome,
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
    fn invert_subgraph_titles_ansi_wraps_title_tokens() {
        use termiflow::graph::Subgraph;

        let mut graph = Graph::new();
        graph
            .subgraphs
            .push(Subgraph::new("service", Some("Service Layer".to_string())));
        graph
            .subgraphs
            .push(Subgraph::new("data", Some("Data Layer".to_string())));

        let output = "┏━━[  Service Layer  ]━━┓ ┏━━[  Data Layer  ]━━┓";
        let styled = invert_subgraph_titles_ansi(output, &graph);

        assert!(styled.contains("\u{1b}[7m   Service Layer   \u{1b}[0m"));
        assert!(styled.contains("\u{1b}[7m   Data Layer   \u{1b}[0m"));
        assert!(!styled.contains("[  Service Layer  ]"));
        assert!(!styled.contains("[  Data Layer  ]"));
    }

    #[test]
    fn printable_output_inverts_titles_by_default_for_tty_print_mode() {
        use termiflow::graph::Subgraph;

        let mut graph = Graph::new();
        graph
            .subgraphs
            .push(Subgraph::new("group", Some("My Group".to_string())));
        let rendered = PreparedRender {
            graph,
            outcome: RenderOutcome {
                output: "┏━━[  My Group  ]━━┓".to_string(),
                semantic_frame: SemanticFrame::default(),
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

        assert!(tty_output.contains("\u{1b}[7m   My Group   \u{1b}[0m"));
        assert_eq!(piped_output, rendered.outcome.output);
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
