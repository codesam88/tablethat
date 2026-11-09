use clap::Parser;
use std::path::PathBuf;
use tablethat_lib as lib;

#[derive(Parser)]
#[command(
    name = "plan",
    version,
    max_term_width = 80,
    about = "Task management with kanban TUI\nCommands can be abbreviated to their first letter (e.g., `plan b` for browse)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file (default: auto-detect)
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Override .plan discovery (schema, task files)
    #[arg(short, long, global = true, value_name = "PATH")]
    root: Option<PathBuf>,

    /// Show done items (hidden by default)
    #[arg(long, global = true)]
    show_done: bool,
}

#[derive(Parser)]
enum Commands {
    /// Kanban board view (default)
    #[command(alias = "k")]
    Kanban {
        /// Filter by status
        #[arg(short, long, value_name = "STATUS")]
        status: Option<String>,

        /// Filter by type
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_: Option<String>,

        /// Filter by priority
        #[arg(short, long, value_name = "PRIORITY")]
        priority: Option<String>,

        /// Filter by area label
        #[arg(short, long, value_name = "AREA")]
        area: Option<String>,

        /// Search slug and body text
        #[arg(short = 'q', long, value_name = "QUERY")]
        search: Option<String>,

        /// Sort key(s), repeatable (status, type, priority, area, slug)
        #[arg(short = 'S', long = "sort", value_name = "FIELD")]
        sort: Vec<String>,
    },

    /// Interactive kanban browser
    #[command(alias = "b")]
    Browse {
        /// Filter by status
        #[arg(short, long, value_name = "STATUS")]
        status: Option<String>,

        /// Filter by type
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_: Option<String>,

        /// Filter by priority
        #[arg(short, long, value_name = "PRIORITY")]
        priority: Option<String>,

        /// Filter by area label
        #[arg(short, long, value_name = "AREA")]
        area: Option<String>,

        /// Search slug and body text
        #[arg(short = 'q', long, value_name = "QUERY")]
        search: Option<String>,
    },

    /// List tasks in a table
    #[command(alias = "l")]
    List {
        /// Filter by status
        #[arg(short, long, value_name = "STATUS")]
        status: Option<String>,

        /// Filter by type
        #[arg(short = 't', long = "type", value_name = "TYPE")]
        type_: Option<String>,

        /// Filter by priority
        #[arg(short, long, value_name = "PRIORITY")]
        priority: Option<String>,

        /// Filter by area label
        #[arg(short, long, value_name = "AREA")]
        area: Option<String>,

        /// Search slug and body text
        #[arg(short = 'q', long, value_name = "QUERY")]
        search: Option<String>,

        /// Sort key(s), repeatable (status, type, priority, area, slug)
        #[arg(short = 'S', long = "sort", value_name = "FIELD")]
        sort: Vec<String>,
    },

    /// Initialize a .plan/ directory with schema and template
    Init,

    /// Create a new task
    #[command(alias = "a")]
    Add {
        /// Task slug (filename without .md)
        slug: String,

        /// Initial status
        #[arg(short, long, default_value = "open", value_name = "STATUS")]
        status: String,

        /// Task type
        #[arg(
            short = 't',
            long = "type",
            default_value = "feature",
            value_name = "TYPE"
        )]
        type_: String,

        /// Priority level
        #[arg(short, long, default_value = "medium", value_name = "PRIORITY")]
        priority: String,

        /// Area label
        #[arg(long, default_value = "", value_name = "AREA")]
        area: String,
    },

    /// Open a task in your editor
    #[command(alias = "o")]
    Open {
        /// Task slug (prefix/substring match)
        slug: String,
    },

    /// Delete a task
    #[command(alias = "d")]
    Delete {
        /// Task slug (prefix/substring match)
        slug: String,
    },

    /// Validate task frontmatter
    Lint {
        /// File or directory to lint (default: .plan tree)
        path: Option<PathBuf>,
    },
    /// Format task markdown (frontmatter + body)
    Format {
        /// Files or directories to format (default: .plan/*.md)
        #[arg(
            value_name = "PATH",
            help = "Files or directories to format (default: .plan/*.md)"
        )]
        paths: Vec<PathBuf>,
        /// Wrap width for markdown body
        #[arg(short, long, value_name = "COLS")]
        width: Option<usize>,
        /// Join short lines into flow before re-splitting
        #[arg(long, overrides_with = "no_reflow")]
        reflow: bool,
        /// Don't join short lines
        #[arg(long, overrides_with = "reflow")]
        no_reflow: bool,
        /// Report files that would change without writing
        #[arg(long)]
        check: bool,
    },
}

fn run(cli: Cli) -> Result<(), String> {
    let mut cfg = lib::Config::try_load("plan", "PLAN_", cli.config.as_deref())?;
    if cli.root.is_some() {
        cfg.root = cli.root;
    }
    if cli.show_done {
        cfg.hide_done = false;
    }
    let root = cfg.root.clone().unwrap_or_else(lib::workspace_root);

    match cli.command {
        Some(Commands::List {
            status,
            type_,
            priority,
            area,
            search,
            sort,
        }) => {
            tablethat_lib::plan::display::list_tasks(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
                false,
                &sort,
            )?;
            Ok(())
        }
        Some(Commands::Kanban {
            status,
            type_,
            priority,
            area,
            search,
            sort,
        }) => {
            tablethat_lib::plan::display::list_tasks(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
                true,
                &sort,
            )?;
            Ok(())
        }
        Some(Commands::Add {
            slug,
            status,
            type_,
            priority,
            area,
        }) => match tablethat_lib::plan::store::create_task(
            &root, &slug, &status, &type_, &priority, &area,
        ) {
            Ok(path) => {
                eprintln!("created {}", path.display());
                Ok(())
            }
            Err(e) => Err(e),
        },
        Some(Commands::Open { slug }) => {
            let path = match tablethat_lib::plan::store::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => return Err(format!("no task matching '{slug}'")),
            };
            tablethat_lib::plan::store::open_task(&path, cfg.pager_width)?;
            Ok(())
        }
        Some(Commands::Delete { slug }) => {
            let path = match tablethat_lib::plan::store::resolve_single_slug(&root, &slug) {
                Some(p) => p,
                None => return Err(format!("no task matching '{slug}'")),
            };
            match tablethat_lib::plan::store::delete_task(&path) {
                Ok(deleted) => {
                    eprintln!("deleted {}", deleted.display());
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Some(Commands::Browse {
            status,
            type_,
            priority,
            area,
            search,
        }) => {
            tablethat_lib::plan::tui::run_tui(
                &root,
                &cfg,
                status.as_deref(),
                type_.as_deref(),
                priority.as_deref(),
                area.as_deref(),
                search.as_deref(),
            )
            .map_err(err_chain)?;
            Ok(())
        }
        Some(Commands::Init) => {
            tablethat_lib::plan::store::init_plan(&root);
            Ok(())
        }
        Some(Commands::Lint { path }) => {
            let mut warnings = Vec::new();
            let result = if let Some(p) = path {
                if p.is_dir() {
                    tablethat_lib::plan::lint::validate_dir(&p, &mut warnings)
                } else if p.is_file() {
                    tablethat_lib::plan::lint::validate_file(&p, &mut warnings)
                } else {
                    return Err(format!("{}: not a file or directory", p.display()));
                }
            } else if root.join(tablethat_lib::PLAN_DIR).is_dir() {
                if tablethat_lib::resolve_file(
                    &root,
                    ".plan",
                    ".schema.json",
                    "schema.json",
                    "plan",
                )
                .is_none()
                {
                    return Err(
                        "error: no schema found (searched .plan/.schema.json, config dir, data dir)"
                            .into(),
                    );
                }
                tablethat_lib::plan::lint::validate_all(&root, &mut warnings)
            } else {
                tablethat_lib::plan::lint::validate_dir(&root, &mut warnings)
            };
            match result {
                Ok(()) => {}
                Err(errors) => {
                    let mut all = errors;
                    all.extend(warnings);
                    return Err(all.join("\n"));
                }
            }
            for w in &warnings {
                eprintln!("{w}");
            }
            Ok(())
        }
        Some(Commands::Format {
            paths,
            width,
            reflow,
            no_reflow,
            check,
        }) => {
            let effective_width = width.unwrap_or(cfg.format.width);
            let effective_reflow = if reflow {
                true
            } else if no_reflow {
                false
            } else {
                cfg.format.reflow
            };
            if paths.is_empty() {
                if check {
                    let tasks_dir = root.join(tablethat_lib::PLAN_DIR);
                    let entries = tablethat_lib::plan::store::read_task_files(&tasks_dir)?;
                    let mut errors: Vec<String> = Vec::new();
                    let mut would_change = 0usize;
                    for entry in &entries {
                        match tablethat_lib::plan::format::format_file(
                            entry,
                            effective_width,
                            effective_reflow,
                            true,
                        ) {
                            Ok(true) => {
                                eprintln!("would format {}", entry.display());
                                would_change += 1;
                            }
                            Ok(false) => {}
                            Err(e) => errors.push(e),
                        }
                    }
                    if !errors.is_empty() {
                        return Err(errors.join("\n"));
                    }
                    if would_change > 0 {
                        return Err(format!("{} file(s) would be reformatted", would_change));
                    }
                    eprintln!("all files already canonical");
                    Ok(())
                } else {
                    match tablethat_lib::plan::format::normalize_all(
                        &root,
                        effective_width,
                        effective_reflow,
                        false,
                    ) {
                        Ok(0) => {
                            eprintln!("all files already canonical");
                            Ok(())
                        }
                        Ok(n) => {
                            eprintln!("fixed {n} file(s)");
                            Ok(())
                        }
                        Err(errors) => Err(errors.join("\n")),
                    }
                }
            } else {
                let (targets, mut errors) =
                    tablethat_lib::plan::store::expand_format_targets(&paths);
                let mut would_change = 0usize;
                for entry in &targets {
                    let is_task = entry.components().any(|c| c.as_os_str() == ".plan");
                    let fmt_res = if is_task {
                        tablethat_lib::plan::format::format_file(
                            entry,
                            effective_width,
                            effective_reflow,
                            check,
                        )
                    } else {
                        tablethat_lib::plan::format::format_general_file(
                            entry,
                            effective_width,
                            effective_reflow,
                            check,
                        )
                    };
                    match fmt_res {
                        Ok(true) => {
                            if check {
                                eprintln!("would format {}", entry.display());
                                would_change += 1;
                            } else {
                                eprintln!("formatted {}", entry.display());
                            }
                        }
                        Ok(false) => {}
                        Err(e) => errors.push(e),
                    }
                }
                if !errors.is_empty() {
                    return Err(errors.join("\n"));
                }
                if check && would_change > 0 {
                    if targets.len() == 1 && would_change == 1 {
                        return Err(format!("{}: would be reformatted", targets[0].display()));
                    }
                    return Err(format!("{} file(s) would be reformatted", would_change));
                }
                Ok(())
            }
        }
        None => {
            // Default view from config
            match cfg.default_view.as_str() {
                "kanban" => {
                    tablethat_lib::plan::display::list_tasks(
                        &root,
                        &cfg,
                        None,
                        None,
                        None,
                        None,
                        None,
                        true,
                        &[],
                    )?;
                }
                _ => {
                    tablethat_lib::plan::display::list_tasks(
                        &root,
                        &cfg,
                        None,
                        None,
                        None,
                        None,
                        None,
                        false,
                        &[],
                    )?;
                }
            }
            Ok(())
        }
    }
}

fn err_chain(e: Box<dyn std::error::Error>) -> String {
    let mut msg = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        cur = s.source();
    }
    msg
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("plan: {e}");
        std::process::exit(1);
    }
}
