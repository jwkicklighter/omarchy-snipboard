//! Development tools. None of them enable plugins or touch clipboard data.
//!
//!   benchmark [--rounds N]          matching and worker round trips (not UI latency)
//!   validate                        manifest validation and qmllint
//!   probe scratch|benchmark [--output PATH]
//!                                   explicitly approved in-shell probes
//!   install | uninstall | unlink-work
//!                                   move the single repository between Work and plugins

use std::fs::{self, DirBuilder};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{DirBuilderExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use snipboard::core::{synthetic_library, synthetic_records};
use snipboard::dev::{self, ID};
use snipboard::search::{Index, RESULT_LIMIT};
use snipboard::storage::Usage;
use snipboard::{source_root, sys};

type Outcome = Result<(), String>;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let result = match arguments.as_slice() {
        ["benchmark", rest @ ..] => option(rest, "--rounds", "30")
            .and_then(|rounds| rounds.parse().map_err(|_| "--rounds must be a number".to_owned()))
            .and_then(benchmark),
        ["validate"] => validate(),
        ["probe", suite @ ("scratch" | "benchmark"), rest @ ..] => {
            let default = std::env::temp_dir().join("snipboard-probe.json");
            option(rest, "--output", &default.to_string_lossy()).and_then(|output| probe(suite, Path::new(&output)))
        }
        [action @ ("install" | "uninstall" | "unlink-work")] => relocate(action),
        _ => Err(include_str!("snipboard-tools.rs")
            .lines()
            .take_while(|line| line.starts_with("//!"))
            .map(|line| line.trim_start_matches("//!").trim_start_matches(' '))
            .collect::<Vec<_>>()
            .join("\n")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn option(rest: &[&str], name: &str, default: &str) -> Result<String, String> {
    match rest {
        [] => Ok(default.to_owned()),
        [flag, value] if *flag == name => Ok((*value).to_owned()),
        _ => Err(format!("expected optional {name} VALUE")),
    }
}

/// An owner-only scratch directory outside the source tree, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(prefix: &str) -> Result<Self, String> {
        let suffix = sys::random_hex(6).map_err(|e| e.to_string())?;
        let path = std::env::temp_dir().join(format!("{prefix}{suffix}"));
        DirBuilder::new().mode(0o700).create(&path).map_err(|e| e.to_string())?;
        Ok(Scratch(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Nearest-rank percentiles in milliseconds.
fn summary(samples: &[f64]) -> Value {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    let rank = |p: f64| {
        let index = ((ordered.len() as f64 * p).ceil() as usize).saturating_sub(1);
        (ordered.get(index).copied().unwrap_or(f64::NAN) * 1000.0).round() / 1000.0
    };
    json!({"n": samples.len(), "p50_ms": rank(0.5), "p95_ms": rank(0.95), "p99_ms": rank(0.99)})
}

fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn platform() -> String {
    // SAFETY: uname fills the zeroed struct we own; fields are NUL-terminated.
    let mut names: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut names) } != 0 {
        return "unknown".into();
    }
    let field = |raw: &[libc::c_char]| {
        let bytes: Vec<u8> = raw.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    format!("{}-{}-{}", field(&names.sysname), field(&names.release), field(&names.machine))
}

fn benchmark(rounds: usize) -> Outcome {
    let records = synthetic_records(&synthetic_library(1000), 10000);
    let payload: usize = records.iter().map(|r| r.body.chars().count()).sum();
    let count = records.len();
    let index = Index::new(records, &Usage::new());
    let queries = ["", "e", "em", "ema", "email", "!eml", "prjnt", "project", "amber", "river", "999", "καλη", "zzzz"];
    let search = |query| index.search(query, "all", RESULT_LIMIT, sys::now()).map(|_| ()).map_err(|e| e.to_string());
    for query in queries {
        search(query)?;
    }
    let mut samples = Vec::new();
    for _ in 0..rounds {
        for query in queries {
            let start = Instant::now();
            search(query)?;
            samples.push(milliseconds(start));
        }
    }

    let data = Scratch::new("snipboard-bench-")?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("snipboard-worker");
    let start = Instant::now();
    let mut worker = Command::new(&executable)
        .arg("--data")
        .arg(&data.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e} (run `make build` first)", executable.display()))?;
    let mut input = worker.stdin.take().expect("piped");
    let mut output = BufReader::new(worker.stdout.take().expect("piped"));
    let mut read = || -> Result<Value, String> {
        let mut line = String::new();
        output.read_line(&mut line).map_err(|e| e.to_string())?;
        serde_json::from_str(&line).map_err(|e| e.to_string())
    };
    read()?;
    let startup = milliseconds(start);
    let (mut trips, mut matches) = (Vec::new(), Vec::new());
    for i in 0..rounds * queries.len() {
        let start = Instant::now();
        writeln!(input, "{}", json!({"op": "search", "seq": i, "query": queries[i % queries.len()]}))
            .map_err(|e| e.to_string())?;
        let result = read()?;
        trips.push(milliseconds(start));
        matches.push(result["matchMs"].as_f64().ok_or("worker search failed")?);
    }
    drop(input);
    worker.wait().map_err(|e| e.to_string())?;
    let report = json!({
        "platform": platform(),
        "matching_engine": "Rust",
        "records": count,
        "payload_characters": payload,
        "index_limit_per_field": snipboard::search::INDEX_LIMIT,
        "queries": queries,
        "matching_only": summary(&samples),
        "worker_matching_only": summary(&matches),
        "sequential_worker_roundtrip_not_visible_latency": summary(&trips),
        "cold_worker_ready_ms": (startup * 1000.0).round() / 1000.0,
        "warm_open_visible_ms": null,
        "input_to_visible_ms": null,
    });
    println!("{}", serde_json::to_string_pretty(&report).expect("serializable"));
    Ok(())
}

/// Read-only manifest/lint validation. The temporary import mapping is outside source.
fn validate() -> Outcome {
    let root = source_root();
    let status = Command::new("omarchy").args(["plugin", "validate"]).arg(&root).status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("manifest validation failed".into());
    }
    let imports = Scratch::new("snipboard-lint-")?;
    symlink("/usr/share/omarchy/shell", imports.0.join("qs")).map_err(|e| e.to_string())?;
    let result = Command::new("/usr/lib/qt6/bin/qmllint")
        .arg("-I")
        .arg(&imports.0)
        .args(["Snipboard.qml", "SbButton.qml", "SbSnippetEditor.qml", "SbPasteHandoff.qml", "SbPreview.qml"].map(|name| root.join(name)))
        .output()
        .map_err(|e| e.to_string())?;
    let output = String::from_utf8_lossy(&result.stdout).into_owned() + &String::from_utf8_lossy(&result.stderr);
    let log = std::env::temp_dir().join("snipboard-qmllint.log");
    fs::write(&log, &output).map_err(|e| e.to_string())?;
    let warnings: Vec<&str> = output.lines().filter(|line| line.starts_with("Warning:")).collect();
    let known = ["missing-property", "uncreatable-type", "signal-handler-parameters"];
    let unexpected = warnings.iter().any(|line| !known.iter().any(|category| line.ends_with(&format!("[{category}]"))));
    let code = result.status.code().unwrap_or(-1);
    println!("Manifest valid. qmllint exit={code}, {} warnings; full output: {}", warnings.len(), log.display());
    println!("Known installed metadata warnings: inline shell QtObject tokens, PanelWindow factory, QProcess enum. No warnings suppressed.");
    if code != 0 || unexpected || output.contains("Error:") {
        println!("{output}");
        return Err("qmllint reported unexpected problems".into());
    }
    Ok(())
}

fn shell(arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("omarchy-shell").arg("shell").args(arguments).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("omarchy-shell shell {} failed", arguments.join(" ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Explicitly approved in-shell scratch/latency probes, never external OS input. Calls
/// fixed Snipboard methods only and writes synthetic diagnostics, never clipboard data.
fn probe(suite: &str, output: &Path) -> Outcome {
    println!("{}", shell(&["summon", ID, "{}"])?);
    let result = shell(&["call", ID, if suite == "scratch" { "selfTest" } else { "benchmark" }, ""])?;
    if !result.starts_with("started") {
        return Err(result);
    }
    println!("{result}");
    thread::sleep(Duration::from_secs(if suite == "scratch" { 7 } else { 28 }));
    let metrics: Value = serde_json::from_str(&shell(&["call", ID, "metrics", ""])?).map_err(|e| e.to_string())?;
    fs::write(output, serde_json::to_string_pretty(&metrics).expect("serializable") + "\n").map_err(|e| e.to_string())?;
    if suite == "scratch" {
        println!("{}", json!({"phase": metrics["testPhase"], "results": metrics["tests"]}));
        let tests = metrics["tests"].as_array().cloned().unwrap_or_default();
        if metrics["testPhase"] != "complete" || tests.len() != 4 || !tests.iter().all(|t| t["passed"] == true) {
            return Err("scratch self-test did not pass".into());
        }
    } else {
        let samples = |key: &str| -> Vec<f64> {
            metrics[key].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()
        };
        let report: serde_json::Map<String, Value> =
            ["inputToModelAssignmentMs", "syntheticInputToFrameSwappedMs", "openToFrameSwappedMs"]
                .into_iter()
                .map(|key| (key.to_owned(), summary(&samples(key))))
                .collect();
        println!("{}", serde_json::to_string_pretty(&report).expect("serializable"));
    }
    println!("Synthetic diagnostics: {}", output.display());
    Ok(())
}

fn relocate(action: &str) -> Outcome {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
    let work = home.join("Work/omarchy-snipboard");
    let installed = home.join(".config/omarchy/plugins").join(ID);
    match action {
        "install" => dev::relocate(&work, &installed, &work),
        "unlink-work" => dev::unlink_work(&work, &installed),
        _ => dev::uninstall(&work, &installed),
    }
    .map_err(|e| e.to_string())?;
    println!("Done. Source and prototype data preserved; activation unchanged.");
    Ok(())
}
