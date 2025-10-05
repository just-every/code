use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

// Note: helper types for decoding packed keys were removed
// because this binary now parses structured logs instead.

fn parse_response_expected(path: &Path) -> Result<Vec<(u64, u64)>> {
    // Returns vector of (out, seq) in the order they appear if we sort by out then seq
    // We only consider events that carry output_index and sequence_number
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let v: Value = serde_json::from_str(&data)?;
    let events = v
        .get("events")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    let mut items: Vec<(u64, u64)> = Vec::new();
    for ev in events {
        let data = ev.get("data");
        if let Some(d) = data {
            let out = d.get("output_index").and_then(|x| x.as_u64());
            let seq = d.get("sequence_number").and_then(|x| x.as_u64());
            if let (Some(out), Some(seq)) = (out, seq) {
                items.push((out, seq));
            }
        }
    }
    items.sort();
    Ok(items)
}

#[derive(Debug, Deserialize)]
struct InsertLog {
    seq: u64,
    ordered: bool,
    req: u64,
    out: u64,
    item_seq: u64,
}

fn parse_tui_inserts(path: &Path) -> Result<Vec<InsertLog>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let re = Regex::new(r"insert window: seq=(?P<seq>\d+) \((?P<kind>[OU]):(?:req=(?P<req>\d+) out=(?P<out>\d+) seq=(?P<iseq>\d+)|(?P<uval>\d+))\)").unwrap();
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(caps) = re.captures(line) {
            let seq: u64 = caps.name("seq").unwrap().as_str().parse().unwrap_or(0);
            let ordered = &caps["kind"] == "O";
            let (req, out_idx, item_seq) = if ordered {
                let req = caps.name("req").unwrap().as_str().parse().unwrap_or(0);
                let out_idx = caps.name("out").unwrap().as_str().parse().unwrap_or(0);
                let iseq = caps.name("iseq").unwrap().as_str().parse().unwrap_or(0);
                (req, out_idx, iseq)
            } else {
                (
                    0,
                    0,
                    caps.name("uval").unwrap().as_str().parse().unwrap_or(0),
                )
            };
            out.push(InsertLog {
                seq,
                ordered,
                req,
                out: out_idx,
                item_seq,
            });
        }
    }
    Ok(out)
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    eprintln!("order-replay: usage: order_replay <response.json> <codex-tui.log>");
    let response = args.next().context("missing response.json path")?;
    let log = args.next().context("missing codex-tui.log path")?;

    let expected = parse_response_expected(Path::new(&response))?;
    let actual = parse_tui_inserts(Path::new(&log))?;

    println!("Expected (first 20):");
    for (i, (out, seq)) in expected.iter().take(20).enumerate() {
        println!("  {:>3}: out={} seq={}", i, out, seq);
    }

    println!("\nActual inserts (first 40):");
    for (i, log) in actual.iter().take(40).enumerate() {
        if log.ordered {
            println!(
                "  {:>3}: O:req={} out={} seq={} (raw={})",
                i, log.req, log.out, log.item_seq, log.seq
            );
        } else {
            println!("  {:>3}: U:{}", i, log.item_seq);
        }
    }

    // Quick check: find first O:req=1 out=2 and O:req=1 out=1 positions
    let pos_out1 = actual
        .iter()
        .position(|l| l.ordered && l.req == 1 && l.out == 1);
    let pos_out2 = actual
        .iter()
        .position(|l| l.ordered && l.req == 1 && l.out == 2);
    if let (Some(p1), Some(p2)) = (pos_out1, pos_out2) {
        println!(
            "\nCheck: first out=1 at {}, first out=2 at {} => {}",
            p1,
            p2,
            if p1 < p2 { "OK" } else { "WRONG" }
        );
    } else {
        println!("\nCheck: missing ordered inserts for out=1 or out=2 in req=1");
    }

    Ok(())
}
