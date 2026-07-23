use analysis::{build_interference_graph, compute_live_intervals};
use anyhow::{Context, Result};
use clap::Parser;
use ir::examples;
use std::fs;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "basicblock")]
    example: String,
    #[arg(long)]
    out: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let f = match args.example.as_str() {
        "trace" => examples::trace()?,
        _ => examples::basicblock()?,
    };

    let ig = build_interference_graph(&f)?;
    let li = compute_live_intervals(&f)?;

    let mut dot = String::new();
    dot.push_str("graph IG {\n  node [shape=circle];\n");

    for v in ig.nodes.iter() {
        // attach interval as label
        let mut label = format!("{:?}", v);
        if let Some(iv) = li.intervals.iter().find(|i| i.v == *v) {
            label = format!("{:?} [{}..{}]", v, (iv.start.0), (iv.end.0));
        }
        dot.push_str(&format!("  \"{:?}\" [label=\"{}\"];\\n", v, label));
    }

    // undirected edges: print each once
    use std::collections::HashSet;
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    for (a, ns) in ig.edges.iter() {
        for b in ns.iter() {
            let (x, y) = (a.0, b.0);
            let key = if x < y { (x, y) } else { (y, x) };
            if seen.insert(key) {
                dot.push_str(&format!("  \"{:?}\" -- \"{:?}\";\\n", a, b));
            }
        }
    }

    dot.push_str("}\n");
    fs::write(&args.out, dot).context("write dot")?;
    println!("Wrote {}", args.out);
    Ok(())
}
