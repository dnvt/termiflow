use criterion::{black_box, criterion_group, criterion_main, Criterion};
use termiflow::{render, RenderOptions, BaseStyle};

fn simple_diagram() -> &'static str {
    "graph TD\nA[Start] --> B[Process]\nB --> C[End]"
}

fn complex_diagram() -> &'static str {
    "graph TD
    A[Gateway] --> B[Auth]
    A --> C[API]
    B --> D[Database]
    C --> D
    D --> E[Cache]
    D --> F[Logger]
    E --> G[Output]
    F --> G"
}

fn large_branching_diagram() -> &'static str {
    "graph TD
    Root[Root] --> A1[Node A1]
    Root --> A2[Node A2]
    Root --> A3[Node A3]
    Root --> A4[Node A4]
    A1 --> B1[Node B1]
    A1 --> B2[Node B2]
    A2 --> B3[Node B3]
    A2 --> B4[Node B4]
    A3 --> B5[Node B5]
    A3 --> B6[Node B6]
    A4 --> B7[Node B7]
    A4 --> B8[Node B8]
    B1 --> C1[End 1]
    B2 --> C1
    B3 --> C2[End 2]
    B4 --> C2
    B5 --> C3[End 3]
    B6 --> C3
    B7 --> C4[End 4]
    B8 --> C4"
}

fn benchmark_simple_render(c: &mut Criterion) {
    let input = simple_diagram();
    c.bench_function("render_simple_td", |b| {
        b.iter(|| render(black_box(input), RenderOptions::default()))
    });
}

fn benchmark_complex_render(c: &mut Criterion) {
    let input = complex_diagram();
    c.bench_function("render_complex_td", |b| {
        b.iter(|| render(black_box(input), RenderOptions::default()))
    });
}

fn benchmark_large_branching(c: &mut Criterion) {
    let input = large_branching_diagram();
    c.bench_function("render_large_branching", |b| {
        b.iter(|| render(black_box(input), RenderOptions::default()))
    });
}

fn benchmark_different_orientations(c: &mut Criterion) {
    let mut group = c.benchmark_group("orientations");
    
    let td_input = "graph TD\nA[Start] --> B[Mid] --> C[End]";
    let lr_input = "graph LR\nA[Start] --> B[Mid] --> C[End]";
    let bt_input = "graph BT\nA[Start] --> B[Mid] --> C[End]";
    let rl_input = "graph RL\nA[Start] --> B[Mid] --> C[End]";
    
    group.bench_function("TD", |b| {
        b.iter(|| render(black_box(td_input), RenderOptions::default()))
    });
    
    group.bench_function("LR", |b| {
        b.iter(|| render(black_box(lr_input), RenderOptions::default()))
    });
    
    group.bench_function("BT", |b| {
        b.iter(|| render(black_box(bt_input), RenderOptions::default()))
    });
    
    group.bench_function("RL", |b| {
        b.iter(|| render(black_box(rl_input), RenderOptions::default()))
    });
    
    group.finish();
}

fn benchmark_different_styles(c: &mut Criterion) {
    let mut group = c.benchmark_group("styles");
    let input = complex_diagram();
    
    group.bench_function("ascii", |b| {
        b.iter(|| render(black_box(input), RenderOptions::new().with_style(BaseStyle::Ascii)))
    });
    
    group.bench_function("unicode", |b| {
        b.iter(|| render(black_box(input), RenderOptions::new().with_style(BaseStyle::Unicode)))
    });
    
    group.bench_function("rounded", |b| {
        b.iter(|| render(black_box(input), RenderOptions::new().with_style(BaseStyle::Rounded)))
    });
    
    group.bench_function("heavy", |b| {
        b.iter(|| render(black_box(input), RenderOptions::new().with_style(BaseStyle::Heavy)))
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_simple_render,
    benchmark_complex_render,
    benchmark_large_branching,
    benchmark_different_orientations,
    benchmark_different_styles
);
criterion_main!(benches);