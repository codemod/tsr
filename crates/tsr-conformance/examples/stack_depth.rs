//! How much stack each stage needs, and on what shape of input.
//!
//! Diagnostic tool, not a suite. `bd tsr-el3` recorded that the printer needs
//! between 6 and 8 MiB in debug on `compiler/binderBinaryExpressionStress`, and
//! left the binder unmeasured. This bisects the minimum surviving stack size for
//! each stage over several pathological shapes, so the numbers behind a stack
//! policy are measured rather than assumed — which matters because the checker
//! will recurse deeper than anything here.
//!
//! Measures `parse` and `parse`+`bind`. The printer is deliberately not included:
//! `bd tsr-el3` already has a figure for it, and the gap worth filling was the
//! binder, recorded there as unmeasured.
//!
//! Method: run the stage on a thread with an explicit stack size and see whether it
//! returns. A stack overflow aborts the *process*, not the thread, so each probe
//! runs as a child process — this binary re-invokes itself with
//! `<shape> <depth> <stage> <stack-bytes>`. Without that, the first overflow ends
//! the run and every later number is silently missing.

use std::process::Command;

/// One input shape, generated rather than taken from the corpus so the nesting
/// depth is a parameter rather than whatever a test happened to contain.
fn source(shape: &str, depth: usize) -> String {
    match shape {
        // `a + a + a + …`. Upstream's own stress case; strada has a trampoline for
        // exactly this and names it in that file's header comment.
        "binary" => format!("const x = {};", "a + ".repeat(depth) + "a"),
        // `((((…))))`. The parser recurses per level here, where it loops for
        // binary operators.
        "parens" => format!("const x = {}a{};", "(".repeat(depth), ")".repeat(depth)),
        // `[[[[…]]]]` as a *type*, which is a different recursion than the
        // expression one and is the shape a deeply generic library produces.
        "arraytype" => format!("let x: {}number{};", "Array<".repeat(depth), ">".repeat(depth)),
        // Nested conditional types — the closest thing here to what the checker
        // will do when it instantiates them.
        "conditional" => {
            let mut text = String::from("type T = ");
            for _ in 0..depth {
                text.push_str("A extends B ? ");
            }
            text.push('C');
            for _ in 0..depth {
                text.push_str(" : D");
            }
            text.push(';');
            text
        }
        _ => unreachable!("unknown shape {shape}"),
    }
}

/// Run one stage over one input, in this process. Returns nothing; the point is
/// whether it comes back at all.
fn run(shape: &str, depth: usize, stage: &str) {
    let text = source(shape, depth);
    let parsed =
        tsr_parser::ParsedFile::parse_with_script_kind(text, tsr_parser::ScriptKind::TypeScript);
    if stage == "parse" {
        std::hint::black_box(parsed.diagnostics().len());
        return;
    }
    parsed.with_ast(|file| {
        let bound = tsr_binder::bind(
            file,
            parsed.nodes(),
            tsr_binder::FileInfo { name: "stress.ts", text: parsed.source() },
        );
        std::hint::black_box(bound.symbols().len());
    });
}

fn main() {
    let mut args = std::env::args().skip(1);
    // Child mode: `stack_depth <shape> <depth> <stage> <stack-bytes>`.
    if let (Some(shape), Some(depth), Some(stage), Some(stack)) =
        (args.next(), args.next(), args.next(), args.next())
    {
        let depth: usize = depth.parse().expect("depth");
        let stack: usize = stack.parse().expect("stack");
        let shape_owned = shape.clone();
        let handle = std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || run(&shape_owned, depth, &stage))
            .expect("spawn");
        handle.join().expect("the stage panicked");
        return;
    }

    let exe = std::env::current_exe().expect("current exe");
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    println!("minimum surviving stack, {profile} profile (KiB)\n");
    println!("{:<14} {:>7}  {:>10}  {:>12}", "shape", "depth", "parse", "parse+bind");

    for shape in ["binary", "parens", "arraytype", "conditional"] {
        for depth in [1_000, 5_000] {
            let mut cells = Vec::new();
            for stage in ["parse", "bind"] {
                // Bisect over powers of two, then refine: a precise byte count is
                // noise, the order of magnitude is the decision.
                let mut survived = None;
                let mut size = 256 * 1024;
                while size <= 512 * 1024 * 1024 {
                    let ok = Command::new(&exe)
                        .args([shape, &depth.to_string(), stage, &size.to_string()])
                        .output()
                        .is_ok_and(|out| out.status.success());
                    if ok {
                        survived = Some(size / 1024);
                        break;
                    }
                    size *= 2;
                }
                cells.push(survived.map_or_else(|| ">512M".to_string(), |kib| format!("{kib}")));
            }
            println!("{shape:<14} {depth:>7}  {:>10}  {:>12}", cells[0], cells[1]);
        }
    }
    println!(
        "\nEach cell is the smallest power-of-two stack on which the stage returned.\n\
         Upstream needs no such number: Go grows a goroutine stack on demand to 1 GiB."
    );
}
