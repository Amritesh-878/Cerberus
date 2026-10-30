use super::*;
use crate::saved::tests::tab;

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

fn spec(project: &str, tab: Option<&str>) -> Spec {
    Spec {
        project: project.to_owned(),
        tab: tab.map(str::to_owned),
    }
}

fn project(id: ProjectId, name: &str, folder: &str) -> SavedProject {
    SavedProject {
        id,
        name: name.to_owned(),
        folder: folder.to_owned(),
    }
}

fn titled(id: TabId, project_id: ProjectId, parent: Option<TabId>, title: &str) -> SavedTab {
    SavedTab {
        title: title.to_owned(),
        ..tab(id, project_id, parent, "claude")
    }
}

fn tree() -> SavedTree {
    SavedTree {
        projects: vec![
            project(1, "alpha", r"C:\work\alpha"),
            project(2, "Alpha", r"C:\old\alpha-copy"),
            project(3, "Beta", r"C:\code\beta-site\"),
            project(4, "gamma", r"C:\x\shared"),
            project(5, "delta", "D:/y/shared"),
        ],
        tabs: vec![
            titled(10, 1, None, "planner"),
            titled(11, 1, Some(10), "coder"),
            titled(12, 1, None, "shell"),
            titled(13, 1, None, "Shell"),
            titled(14, 3, None, "server"),
        ],
        next_id: 15,
    }
}

fn target(project_id: ProjectId, tab_id: Option<TabId>) -> Target {
    Target { project_id, tab_id }
}

fn opened(spec: &Spec) -> Target {
    resolve(&tree(), spec).unwrap().target()
}

#[test]
fn no_arguments_start_the_app() {
    assert_eq!(parse(&[]), Some(Command::Start));
}

#[test]
fn open_takes_a_project_and_an_optional_tab() {
    assert_eq!(
        parse(&args("open alpha")),
        Some(Command::Open(spec("alpha", None)))
    );
    assert_eq!(
        parse(&args("open alpha/planner")),
        Some(Command::Open(spec("alpha", Some("planner"))))
    );
    assert_eq!(
        parse(&args("open alpha/")),
        Some(Command::Open(spec("alpha", None)))
    );
}

#[test]
fn unquoted_words_after_open_are_one_name() {
    assert_eq!(
        parse(&args("open my app / first planner")),
        Some(Command::Open(spec("my app", Some("first planner"))))
    );
}

#[test]
fn anything_else_is_a_usage_error() {
    assert_eq!(parse(&args("open")), None);
    assert_eq!(parse(&args("open /planner")), None);
    assert_eq!(parse(&args("start alpha")), None);
}

#[test]
fn an_exact_name_wins_over_one_that_differs_in_case() {
    assert_eq!(opened(&spec("alpha", None)), target(1, None));
    assert_eq!(opened(&spec("Alpha", None)), target(2, None));
}

#[test]
fn a_name_matches_in_any_case() {
    assert_eq!(opened(&spec("BETA", None)), target(3, None));
}

#[test]
fn a_folder_name_matches_when_no_project_name_does() {
    assert_eq!(opened(&spec("beta-site", None)), target(3, None));
    assert_eq!(opened(&spec("ALPHA-COPY", None)), target(2, None));
}

#[test]
fn a_name_shared_by_two_projects_lists_both() {
    let miss = resolve(&tree(), &spec("shared", None)).unwrap_err();

    assert_eq!(
        miss,
        Miss::Several {
            what: What::Project,
            query: "shared".to_owned(),
            matches: vec![
                r"gamma (C:\x\shared)".to_owned(),
                "delta (D:/y/shared)".to_owned()
            ],
        }
    );
    assert_eq!(
        miss.to_string(),
        "\"shared\" matches more than one project:\n  gamma (C:\\x\\shared)\n  delta (D:/y/shared)\nRename one to tell them apart."
    );
}

#[test]
fn an_unknown_project_lists_the_close_names() {
    let miss = resolve(&tree(), &spec("alpah", None)).unwrap_err();

    assert!(matches!(&miss, Miss::Nothing { close, .. } if *close == ["alpha", "Alpha"]));
    assert_eq!(
        miss.to_string(),
        "No project matches \"alpah\". Close matches:\n  alpha\n  Alpha"
    );
}

#[test]
fn an_unknown_project_with_nothing_close_lists_every_project() {
    let miss = resolve(&tree(), &spec("zzzzzz", None)).unwrap_err();

    assert_eq!(
        miss.to_string(),
        "No project matches \"zzzzzz\". Known:\n  alpha\n  Alpha\n  Beta\n  gamma\n  delta"
    );
}

#[test]
fn an_empty_tree_says_nothing_is_saved() {
    let miss = resolve(&SavedTree::default(), &spec("alpha", None)).unwrap_err();

    assert_eq!(
        miss.to_string(),
        "No project matches \"alpha\". There are none saved yet."
    );
}

#[test]
fn a_tab_matches_by_title_in_its_project_only() {
    assert_eq!(opened(&spec("alpha", Some("planner"))), target(1, Some(10)));
    assert_eq!(opened(&spec("alpha", Some("CODER"))), target(1, Some(11)));
    assert_eq!(opened(&spec("beta", Some("server"))), target(3, Some(14)));
    assert!(resolve(&tree(), &spec("beta", Some("planner"))).is_err());
}

#[test]
fn an_exact_title_wins_and_a_looser_one_shared_by_two_tabs_lists_both() {
    assert_eq!(opened(&spec("alpha", Some("Shell"))), target(1, Some(13)));

    let miss = resolve(&tree(), &spec("alpha", Some("SHELL"))).unwrap_err();

    assert_eq!(
        miss.to_string(),
        "\"SHELL\" matches more than one tab in alpha:\n  shell\n  Shell\nRename one to tell them apart."
    );
}

#[test]
fn an_unknown_tab_lists_the_close_titles() {
    let miss = resolve(&tree(), &spec("alpha", Some("planer"))).unwrap_err();

    assert_eq!(
        miss.to_string(),
        "No tab in alpha matches \"planer\". Close matches:\n  planner"
    );
}

fn no_app(_: &[String]) -> io::Result<Option<(i32, String)>> {
    Ok(None)
}

#[test]
fn a_start_runs_the_app_without_reading_the_tree() {
    let checked = check(&[], no_app, || panic!("read the tree for a plain start"));

    assert_eq!(checked, Checked::Run(Command::Start, None));
}

#[test]
fn a_match_runs_the_app_and_names_what_it_opens() {
    let checked = check(&args("open alpha/planner"), no_app, || Ok(tree()));

    assert_eq!(
        checked,
        Checked::Run(
            Command::Open(spec("alpha", Some("planner"))),
            Some("Opening alpha / planner".to_owned())
        )
    );
}

#[test]
fn a_miss_exits_with_one() {
    let unknown = check(&args("open nowhere"), no_app, || Ok(tree()));
    let ambiguous = check(&args("open shared"), no_app, || Ok(tree()));
    let unreadable = check(&args("open alpha"), no_app, || Err(TreeError::InUse));

    assert!(matches!(unknown, Checked::Exit(MISSED, _)));
    assert!(matches!(ambiguous, Checked::Exit(MISSED, _)));
    assert_eq!(
        unreadable,
        Checked::Exit(
            MISSED,
            "another Cerberus window has the saved tabs open".to_owned()
        )
    );
}

#[test]
fn bad_usage_exits_with_two_without_reading_the_tree() {
    let checked = check(&args("open"), no_app, || {
        panic!("read the tree for bad usage")
    });

    assert_eq!(checked, Checked::Exit(BAD_USAGE, USAGE.to_owned()));
}

#[test]
fn a_running_app_answers_instead_of_the_file() {
    let mut asked = Vec::new();
    let checked = check(
        &args("open alpha/brand new"),
        |args| {
            asked = args.to_vec();
            Ok(Some((0, "Opening alpha / brand new".to_owned())))
        },
        || panic!("read the file while an app answered"),
    );

    assert_eq!(asked, args("open alpha/brand new"));
    assert_eq!(
        checked,
        Checked::Exit(0, "Opening alpha / brand new".to_owned())
    );
}

#[test]
fn a_running_app_that_finds_nothing_exits_with_its_code() {
    let checked = check(
        &args("open nowhere"),
        |_| Ok(Some((MISSED, "No project matches \"nowhere\".".to_owned()))),
        || panic!("read the file while an app answered"),
    );

    assert!(matches!(checked, Checked::Exit(MISSED, _)));
}

#[test]
fn an_app_that_cannot_be_reached_is_a_miss() {
    let checked = check(
        &args("open alpha"),
        |_| Err(io::Error::other("pipe broke")),
        || Ok(tree()),
    );

    assert_eq!(
        checked,
        Checked::Exit(
            MISSED,
            "could not reach the running Cerberus: pipe broke".to_owned()
        )
    );
}
