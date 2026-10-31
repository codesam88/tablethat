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
    for slug in ["../evil", "a/b", "Foo_Bar", "a b", ""] {
        let out = r.run(r.plan().arg("add").arg(slug));
        assert!(!out.status.success(), "slug {slug:?} must fail");
    }
    // Invariant: nothing escaped .plan, nothing created inside it either.
    assert!(!r.path().join("evil.md").exists());
    let count = std::fs::read_dir(r.path().join(".plan"))
        .expect("read")
        .count();
    assert_eq!(count, 2, "only schema + template may exist");
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
    let out = r.run(r.plan().args(["list", "-s", "blocked"]));
    assert!(out.status.success(), "empty result is not an error");
    let text = String::from_utf8(out.stdout).expect("utf8");
    assert!(text.contains("no tasks match"));
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
