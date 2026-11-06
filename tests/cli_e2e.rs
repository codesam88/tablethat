//! CLI end-to-end tests: real binaries, temp roots, no snapshots.
//!
//! Rule: assert functional properties and invariants (exit codes, file
//! existence, structural facts about synthesized input) — never exact
//! stdout/stderr text, which is formatting detail. Exact matches appear
//! only for values the test itself synthesized (slugs, field values).
//!
//! Isolation: every test gets a fresh temp root used as `--root` AND cwd
//! (blocks `workspace_root()` upward search into the real repo), `HOME` +
//! `XDG_CONFIG_HOME` pointed at temp dirs (blocks `~/.config` leakage),
//! and `PLAN_*` / `GLOSS_*` / `EDITOR` stripped from the child env
//! (blocks figment env-layer leakage).
//!
//! Excluded (need a tty/pty): `plan open/browse/kanban`, gloss TUI modes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Root {
    dir: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl Root {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temp root"),
            home: tempfile::tempdir().expect("temp home"),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn cmd(&self, bin: &str) -> Command {
        let exe = match bin {
            "plan" => env!("CARGO_BIN_EXE_plan"),
            "gloss" => env!("CARGO_BIN_EXE_gloss"),
            _ => panic!("unknown bin {bin}"),
        };
        let mut c = Command::new(exe);
        c.current_dir(self.path());
        c.env("HOME", self.home.path());
        c.env("XDG_CONFIG_HOME", self.home.path());
        for (k, _) in std::env::vars() {
            if k.starts_with("PLAN_") || k.starts_with("GLOSS_") || k == "EDITOR" {
                c.env_remove(&k);
            }
        }
        c
    }

    fn plan(&self) -> Command {
        let mut c = self.cmd("plan");
        c.arg("--root").arg(self.path());
        c
    }

    fn run(&self, c: &mut Command) -> Output {
        c.output().expect("spawn binary")
    }

    fn task_path(&self, slug: &str) -> PathBuf {
        self.path().join(".plan").join(format!("{slug}.md"))
    }

    fn frontmatter(&self, slug: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(self.task_path(slug)).expect("read task file");
        let fm: Vec<&str> = text
            .lines()
            .skip(1)
            .take_while(|l| l.trim() != "---")
            .collect();
        serde_saphyr::from_str(&fm.join("\n")).expect("frontmatter parses")
    }
}

// --- plan init / add -------------------------------------------------------

#[test]
fn init_creates_plan_dir_with_schema_and_template() {
    let r = Root::new();
    let out = r.run(r.plan().arg("init"));
    assert!(out.status.success(), "init must succeed");
    assert!(r.path().join(".plan").is_dir());
    // Schema must be valid JSON (structure, not pinned contents).
    let schema = std::fs::read_to_string(r.path().join(".plan/.schema.json")).expect("schema");
    let v: serde_json::Value = serde_json::from_str(&schema).expect("schema parses");
    assert!(v.get("properties").is_some());
    assert!(r.path().join(".plan/.TEMPLATE.md").is_file());
}

#[test]
fn add_round_trips_declared_fields() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    let out = r.run(
        r.plan()
            .args(["add", "my-task", "-s", "open", "-t", "bug", "-p", "high"])
            .arg("--area")
            .arg("backend"),
    );
    assert!(out.status.success(), "add must succeed");
    assert!(r.task_path("my-task").is_file());
    // Values are test-synthesized, so exact match is legitimate.
    let fm = r.frontmatter("my-task");
    assert_eq!(fm["status"], "open");
    assert_eq!(fm["type"], "bug");
    assert_eq!(fm["priority"], "high");
    assert_eq!(fm["area"], "backend");
}

#[test]
fn add_rejects_bad_slugs_without_touching_fs() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    // Schema + template must exist.
    assert!(r.path().join(".plan/.schema.json").is_file());
    assert!(r.path().join(".plan/.TEMPLATE.md").is_file());
    for slug in ["../evil", "a/b", "Foo_Bar", "a b", ""] {
        let out = r.run(r.plan().arg("add").arg(slug));
        assert!(!out.status.success(), "slug {slug:?} must fail");
        // No task file created for any rejected slug.
        assert!(
            !r.task_path(slug).exists(),
            "rejected slug {slug:?} must not create a file"
        );
    }
}

#[test]
fn add_duplicate_fails_and_preserves_original() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "dup"])).status.success());
    let before = std::fs::read_to_string(r.task_path("dup")).expect("read");
    let out = r.run(r.plan().args(["add", "dup"]));
    assert!(!out.status.success(), "duplicate add must fail");
    let after = std::fs::read_to_string(r.task_path("dup")).expect("read");
    assert_eq!(before, after, "failed add must not clobber");
}

// --- plan list / lint ------------------------------------------------------

#[test]
fn list_shows_matching_tasks_and_hides_others() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(
        r.run(r.plan().args(["add", "visible-bug", "-s", "open"]))
            .status
            .success()
    );
    // show_done=false hides `done` even with a matching status filter.
    assert!(
        r.run(r.plan().args(["add", "hidden-done", "-s", "done"]))
            .status
            .success()
    );

    let all = r.run(r.plan().arg("list"));
    assert!(all.status.success());
    let text = String::from_utf8(all.stdout).expect("utf8");
    assert!(text.contains("visible-bug"), "synthesized slug must appear");
    assert!(
        !text.contains("hidden-done"),
        "done must be hidden by default"
    );

    let filtered = r.run(r.plan().args(["list", "-s", "open"]));
    assert!(filtered.status.success());
    let text = String::from_utf8(filtered.stdout).expect("utf8");
    assert!(text.contains("visible-bug"));
    assert!(!text.contains("hidden-done"));
}

#[test]
fn list_on_empty_tree_reports_no_match() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    // Synthesize a task so there is a slug to be absent from a non-matching filter.
    assert!(
        r.run(r.plan().args(["add", "present-slug"]))
            .status
            .success()
    );
    let out = r.run(r.plan().args(["list", "-s", "blocked"]));
    assert!(out.status.success(), "empty result is not an error");
    let text = String::from_utf8(out.stdout).expect("utf8");
    // Empty result must not contain the synthesized slug.
    assert!(
        !text.contains("present-slug"),
        "non-matching slug must not appear"
    );
}

#[test]
fn lint_passes_clean_tree_and_fails_corrupt_file() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "good"])).status.success());
    assert!(r.run(r.plan().arg("lint")).status.success());

    // Corrupt one file's status (exit code is the contract, not the message).
    let mut text = std::fs::read_to_string(r.task_path("good")).expect("read");
    text = text.replacen("status: open", "status: nonsense", 1);
    std::fs::write(r.task_path("good"), text).expect("write");
    assert!(!r.run(r.plan().arg("lint")).status.success());
}

// --- plan format / delete --------------------------------------------------

#[test]
fn format_is_idempotent_and_orders_fields() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "fmt-me"])).status.success());

    // Canonical input is already a fixpoint.
    assert!(r.run(r.plan().arg("format")).status.success());
    let after_first = std::fs::read_to_string(r.task_path("fmt-me")).expect("read");
    assert!(r.run(r.plan().arg("format")).status.success());
    let after_second = std::fs::read_to_string(r.task_path("fmt-me")).expect("read");
    assert_eq!(after_first, after_second, "format must be idempotent");

    // Scrambled field order gets reordered: status < type < priority < area.
    let scrambled = "---\narea: backend\npriority: high\ntype: bug\nstatus: open\n---\n\nbody\n";
    std::fs::write(r.task_path("fmt-me"), scrambled).expect("write");
    assert!(r.run(r.plan().arg("format")).status.success());
    let fixed = std::fs::read_to_string(r.task_path("fmt-me")).expect("read");
    let pos = |key: &str| fixed.find(&format!("\n{key}:")).unwrap_or(usize::MAX);
    assert!(pos("status") < pos("type"));
    assert!(pos("type") < pos("priority"));
    assert!(pos("priority") < pos("area"));
    assert!(fixed.contains("body"), "body must survive formatting");
}

#[test]
fn add_rejects_newline_field_values() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    let out = r.run(r.plan().args(["add", "inj"]).arg("--area").arg("a\nb: c"));
    assert!(!out.status.success(), "newline value must fail");
    assert!(!r.task_path("inj").exists());
}

#[test]
fn format_width_cli_override() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "wide"])).status.success());

    let long = "the quick brown fox jumps over the lazy dog and keeps on going forever and ever";

    // First pass: narrow width wraps the long prose line.
    let scrambled = format!(
        "---\nstatus: open\ntype: bug\npriority: high\narea: backend\n---\n\n{}\n",
        long
    );
    std::fs::write(r.task_path("wide"), scrambled).expect("write");
    let out = r.run(
        r.plan()
            .args(["format", "--width", "20"])
            .arg(r.task_path("wide")),
    );
    assert!(out.status.success(), "format --width 20 must succeed");

    let text = std::fs::read_to_string(r.task_path("wide")).expect("read");
    // Extract lines after the second '---'
    let mut after = Vec::new();
    let mut dashes = 0;
    for line in text.lines() {
        if line.trim() == "---" {
            dashes += 1;
            if dashes == 2 {
                // subsequent lines are body
            }
            continue;
        }
        if dashes >= 2 {
            after.push(line);
        }
    }
    let nonempty: Vec<&str> = after
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();
    assert!(
        nonempty.len() > 1,
        "narrow width should wrap into multiple lines"
    );
    for line in &nonempty {
        assert!(
            line.trim_end().len() <= 20,
            "wrapped line exceeds 20 cols: {line:?}"
        );
    }

    // Second pass: wide width keeps it on (at least) one long line.
    let scrambled2 = format!(
        "---\nstatus: open\ntype: bug\npriority: high\narea: backend\n---\n\n{}\n",
        long
    );
    std::fs::write(r.task_path("wide"), scrambled2).expect("write");
    let out = r.run(
        r.plan()
            .args(["format", "--width", "80"])
            .arg(r.task_path("wide")),
    );
    assert!(out.status.success(), "format --width 80 must succeed");

    let text2 = std::fs::read_to_string(r.task_path("wide")).expect("read");
    let mut after2 = Vec::new();
    let mut dashes2 = 0;
    for line in text2.lines() {
        if line.trim() == "---" {
            dashes2 += 1;
            continue;
        }
        if dashes2 >= 2 {
            after2.push(line);
        }
    }
    let nonempty2: Vec<&str> = after2
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();
    assert!(
        nonempty2.iter().any(|l| l.trim_end().len() > 20),
        "wide width should keep a long line: {nonempty2:?}"
    );
}

#[test]
fn format_width_config_fallback_then_cli_wins() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "layered"])).status.success());

    // Long single-line body synthesized by the test; comfortably wider
    // than either width used below so the outcome distinguishes them.
    let line = "word ".repeat(30).trim_end().to_string();
    let scrambled = format!(
        "---\nstatus: open\ntype: bug\npriority: high\narea: backend\n---\n\n{}\n",
        line
    );
    let body_lines = |slug: &str| -> Vec<String> {
        let text = std::fs::read_to_string(r.task_path(slug)).expect("read");
        let mut dashes = 0;
        text.lines()
            .filter(|l| {
                if l.trim() == "---" {
                    dashes += 1;
                    return false;
                }
                dashes >= 2 && !l.trim().is_empty()
            })
            .map(|l| l.trim_end().to_string())
            .collect()
    };

    // No --width: the env-provided width governs (config layer fallback).
    std::fs::write(r.task_path("layered"), &scrambled).expect("write");
    let out = r.run(
        r.plan()
            .args(["format"])
            .arg(r.task_path("layered"))
            .env("PLAN_FORMAT__WIDTH", "25"),
    );
    assert!(out.status.success(), "format must succeed");
    let wrapped = body_lines("layered");
    assert!(wrapped.len() > 1, "narrow config width should wrap");
    assert!(
        wrapped.iter().all(|l| l.len() <= 25),
        "no --width must honor config width: {wrapped:?}"
    );

    // Same env, explicit --width: CLI wins over config.
    std::fs::write(r.task_path("layered"), &scrambled).expect("write");
    let out = r.run(
        r.plan()
            .args(["format", "--width", "100"])
            .arg(r.task_path("layered"))
            .env("PLAN_FORMAT__WIDTH", "25"),
    );
    assert!(out.status.success(), "format must succeed");
    let wide = body_lines("layered");
    assert!(
        wide.iter().any(|l| l.len() > 25),
        "explicit --width must override config width: {wide:?}"
    );

    // Fixpoint: formatting formatted output changes nothing.
    let once = std::fs::read_to_string(r.task_path("layered")).expect("read");
    let out = r.run(
        r.plan()
            .args(["format", "--width", "100"])
            .arg(r.task_path("layered"))
            .env("PLAN_FORMAT__WIDTH", "25"),
    );
    assert!(out.status.success(), "format must succeed");
    let twice = std::fs::read_to_string(r.task_path("layered")).expect("read");
    assert_eq!(once, twice, "format must be idempotent");
}

#[test]
fn format_reflow_cli_override() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "reflowy"])).status.success());

    let long = "word ".repeat(40).trim_end().to_string();
    let body = format!("first short line\nsecond short line\n{}\n", long);

    // --reflow joins short lines.
    let scrambled = format!(
        "---\nstatus: open\ntype: bug\npriority: high\narea: backend\n---\n\n{}",
        body
    );
    std::fs::write(r.task_path("reflowy"), scrambled).expect("write");
    let out = r.run(
        r.plan()
            .args(["format", "--reflow"])
            .arg(r.task_path("reflowy")),
    );
    assert!(out.status.success(), "format --reflow must succeed");
    let text = std::fs::read_to_string(r.task_path("reflowy")).expect("read");
    assert!(
        text.contains("first short line second short line"),
        "--reflow must join short lines: {text:?}"
    );

    // --no-reflow does NOT join.
    let scrambled2 = format!(
        "---\nstatus: open\ntype: bug\npriority: high\narea: backend\n---\n\n{}",
        body
    );
    std::fs::write(r.task_path("reflowy"), scrambled2).expect("write");
    let out = r.run(
        r.plan()
            .args(["format", "--no-reflow"])
            .arg(r.task_path("reflowy")),
    );
    assert!(out.status.success(), "format --no-reflow must succeed");
    let text2 = std::fs::read_to_string(r.task_path("reflowy")).expect("read");
    assert!(
        !text2.contains("first short line second short line"),
        "--no-reflow must not join short lines: {text2:?}"
    );
}

#[test]
fn format_check_does_not_write() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "chk"])).status.success());

    let scrambled = "---\npriority: high\nstatus: open\ntype: bug\n---\n\nbody\n".to_string();
    std::fs::write(r.task_path("chk"), scrambled.clone()).expect("write");
    let before = std::fs::read(r.task_path("chk")).expect("read");

    let out = r.run(r.plan().args(["format", "--check"]).arg(r.task_path("chk")));
    assert!(!out.status.success(), "check on dirty must fail");
    let after = std::fs::read(r.task_path("chk")).expect("read");
    assert_eq!(before, after, "check must not write");

    // format fixes
    let out = r.run(r.plan().arg("format").arg(r.task_path("chk")));
    assert!(out.status.success(), "format must succeed");

    // check again clean
    let out = r.run(r.plan().args(["format", "--check"]).arg(r.task_path("chk")));
    assert!(out.status.success(), "check on clean must succeed");
}

#[test]
fn format_nonexistent_path_is_error() {
    let r = Root::new();
    let out = r.run(r.plan().arg("format").arg(r.path().join("nope")));
    assert!(!out.status.success(), "bogus path must fail");
}

#[test]
fn delete_removes_task_and_fails_when_missing() {
    let r = Root::new();
    assert!(r.run(r.plan().arg("init")).status.success());
    assert!(r.run(r.plan().args(["add", "gone"])).status.success());
    assert!(r.run(r.plan().args(["delete", "gone"])).status.success());
    assert!(!r.task_path("gone").exists());
    assert!(!r.run(r.plan().args(["delete", "gone"])).status.success());
}

// --- gloss filter ----------------------------------------------------------

fn gloss_stdin(r: &Root, args: &[&str], stdin: &str) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = r
        .cmd("gloss")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn gloss");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

#[test]
fn gloss_filter_renders_prose_without_color() {
    let r = Root::new();
    let out = gloss_stdin(&r, &["--width", "80"], "# Hello\n\nworld of prose\n");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("utf8");
    assert!(text.contains("Hello"), "heading text must render");
    assert!(text.contains("world of prose"));
    assert!(
        !text.contains('\u{1b}'),
        "piped stdout must not carry ANSI escapes"
    );
}

#[test]
fn gloss_filter_respects_width_for_prose() {
    let r = Root::new();
    let input = "one two three four five six seven eight nine ten eleven twelve\n";
    let out = gloss_stdin(&r, &["--width", "20"], input);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("utf8");
    for line in text.lines() {
        assert!(
            line.trim_end().len() <= 20,
            "wrapped line exceeds width: {line:?}"
        );
    }
}

#[test]
fn gloss_rejects_nonexistent_path() {
    let r = Root::new();
    let out = r.run(r.cmd("gloss").arg(r.path().join("nope.md")));
    assert!(!out.status.success());
}

#[test]
fn gloss_filter_table_borders_respect_ascii_flag() {
    let r = Root::new();
    let input = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
    let unicode = gloss_stdin(&r, &["--width", "80"], input);
    assert!(unicode.status.success());
    let utext = String::from_utf8(unicode.stdout).expect("utf8");
    assert!(utext.contains('│'), "unicode default must use box vertical");

    let ascii = gloss_stdin(&r, &["--width", "80", "--ascii"], input);
    assert!(ascii.status.success());
    let atext = String::from_utf8(ascii.stdout).expect("utf8");
    assert!(atext.contains('|'), "ascii mode must use pipe vertical");
    assert!(
        atext.contains('='),
        "ascii mode must use equals header fill"
    );
}
