use crossterm::event::KeyModifiers;
use rstest::rstest;

use super::*;

const WIDTH: usize = 50;
const URL: &str = "https://github.com/acme/web-shop.git";

fn file(path: &str, size: u64, too_big: bool) -> IgnoredFile {
    IgnoredFile { path: path.into(), size, mode: 0o644, too_big }
}

fn opened() -> CloneForm {
    CloneForm::new(Opening {
        project: "shop".into(),
        machine: "vps".into(),
        folder: "~/code/".into(),
        root: "/here/shop".into(),
        url: URL.into(),
    })
}

fn listed() -> CloneForm {
    let mut form = opened();
    form.listed(Ok(Listing {
        files: vec![file(".env", 120, false), file("data/big.sqlite", 9 << 20, true), file("local.toml", 2048, false)],
        ..Listing::default()
    }));
    form
}

fn press(form: &mut CloneForm, code: KeyCode) -> Step {
    form.key(KeyEvent::new(code, KeyModifiers::NONE), WIDTH)
}

fn texts(form: &CloneForm) -> Vec<String> {
    form.lines().into_iter().map(|line| line.text).collect()
}

mod opening {
    use super::*;

    #[test]
    fn names_the_project_the_machine_and_where_the_clone_goes() {
        let form = opened();

        assert_eq!(form.title(), "Clone shop on vps");
        assert_eq!(form.destination(), "~/code/web-shop");
        assert_eq!(texts(&form), [READING]);
    }

    #[test]
    fn follows_the_url_as_it_is_edited() {
        let mut form = opened();
        form.url = Editor::new("git@github.com:acme/api.git");

        assert_eq!(form.destination(), "~/code/api");
        form.url = Editor::new("");
        assert_eq!(form.destination(), "~/code/…");
    }
}

mod the_files {
    use super::*;

    #[test]
    fn are_listed_unticked_and_one_too_big_has_no_box() {
        let form = listed();

        assert_eq!(
            texts(&form),
            ["[ ] .env  120 B", "    data/big.sqlite  9 MB · too big to copy", "[ ] local.toml  2 KB"]
        );
        assert_eq!(form.lines().iter().map(|line| line.file).collect::<Vec<_>>(), [Some(0), None, Some(2)]);
        assert_eq!(form.ticked(), Vec::<IgnoredFile>::new());
    }

    #[test]
    fn are_ticked_and_unticked_with_a_click_but_never_one_too_big() {
        let mut form = listed();

        form.toggle(2);
        form.toggle(1);
        assert_eq!(form.ticked(), [file("local.toml", 2048, false)]);
        form.toggle(2);
        assert_eq!(form.ticked(), Vec::<IgnoredFile>::new());
    }

    #[test]
    fn refuse_a_tick_past_the_total_a_clone_takes() {
        let mut form = opened();
        let big = |n: usize| file(&format!("{n}.bin"), MAX_TOTAL_BYTES / 2, false);
        form.listed(Ok(Listing { files: vec![big(0), big(1), file(".env", 1, false)], ..Listing::default() }));

        form.toggle(0);
        form.toggle(1);
        form.toggle(2);

        assert_eq!(form.ticked(), [big(0), big(1)]);
        assert_eq!(form.error.as_deref(), Some("over the 8 MiB a clone takes"));
        form.toggle(1);
        form.toggle(2);
        assert_eq!((form.ticked(), form.error), (vec![big(0), file(".env", 1, false)], None));
    }

    #[test]
    fn say_how_many_were_left_out() {
        let mut form = opened();
        form.listed(Ok(Listing { files: vec![file(".env", 120, false)], left_out: 3, ..Listing::default() }));

        assert_eq!(texts(&form), ["[ ] .env  120 B", "… and 3 more not offered (200 at most)"]);
        assert_eq!(form.lines()[1].tone, Tone::Muted);
    }

    #[rstest]
    #[case::none(Ok(Listing::default()), NONE, Tone::Muted)]
    #[case::a_failure(Err("not a git repository".into()), "not a git repository", Tone::Red)]
    fn say_when_there_is_nothing_to_tick(
        #[case] listed: Result<Listing, String>,
        #[case] said: &str,
        #[case] tone: Tone,
    ) {
        let mut form = opened();
        form.listed(listed);

        assert_eq!(form.lines(), [Line { text: said.into(), tone, file: None }]);
    }
}

mod cloning {
    use super::*;

    #[test]
    fn sends_only_the_ticked_files() {
        let mut form = listed();
        form.toggle(0);

        assert_eq!(form.press(CLONE), Step::Clone { url: URL.into(), files: vec![file(".env", 120, false)] });
        assert_eq!(
            press(&mut form, KeyCode::Enter),
            Step::Clone { url: URL.into(), files: vec![file(".env", 120, false)] }
        );
    }

    #[test]
    fn skips_the_files_even_when_some_are_ticked() {
        let mut form = listed();
        form.toggle(0);

        assert_eq!(form.press(SKIP_FILES), Step::Clone { url: URL.into(), files: Vec::new() });
    }

    #[test]
    fn needs_a_url() {
        let mut form = opened();
        form.url = Editor::new("  ");

        assert_eq!(form.press(CLONE), Step::None);
        assert_eq!(form.error.as_deref(), Some(NO_URL));
    }

    #[test]
    fn does_nothing_more_while_it_clones() {
        let mut form = listed();
        form.cloning = true;

        assert_eq!(form.press(CLONE), Step::None);
        form.toggle(0);
        assert_eq!(form.ticked(), Vec::<IgnoredFile>::new());
        assert_eq!((form.hint(), form.dimmed()), ("cloning on vps…".to_string(), Some(CLONE)));
        assert_eq!(press(&mut form, KeyCode::Esc), Step::Back);
    }

    #[test]
    fn goes_back_without_sending_anything() {
        let mut form = listed();

        assert_eq!(form.press(BACK), Step::Back);
    }
}

mod pasting {
    use super::*;

    #[test]
    fn puts_a_url_on_one_line_and_clears_the_error() {
        let mut form = opened();
        form.url = Editor::new("");
        assert_eq!(form.press(CLONE), Step::None);

        form.paste("\n  git@github.com:acme/api.git\r\n", WIDTH);

        assert_eq!((form.url.text(), form.error.as_deref()), ("git@github.com:acme/api.git".to_string(), None));
        assert_eq!(form.destination(), "~/code/api");
    }

    #[test]
    fn changes_nothing_while_it_clones() {
        let mut form = listed();
        form.cloning = true;

        form.paste("/more", WIDTH);

        assert_eq!(form.url.text(), URL);
    }
}

mod the_intro {
    use super::*;

    #[test]
    fn says_this_computer_cannot_read_the_ignored_files_when_it_runs_windows() {
        assert_eq!(
            opened().intro_on(false),
            "vps clones it from the URL, without git-ignored files (such as .env): this computer runs Windows, \
             where reading them to copy is not supported yet."
        );
    }

    #[test]
    fn offers_the_files_to_tick_where_this_computer_reads_them() {
        assert!(opened().intro_on(true).contains("then copies the git-ignored files you tick"));
    }
}

mod the_note {
    use super::*;

    fn unwritten(path: &str, why: &str) -> Unwritten {
        Unwritten { path: PathBuf::from(path), why: why.into() }
    }

    #[rstest]
    #[case::none(&[], "cloned web-shop on vps")]
    #[case::one(&[("local.toml", "it is tracked there")], "cloned web-shop on vps · not copied: local.toml (it is tracked there)")]
    #[case::a_blank_path(&[("", "it is tracked there"), (" ", "x")], "cloned web-shop on vps")]
    #[case::an_invisible_path(&[("\u{FEFF}\u{200B}", "x")], "cloned web-shop on vps")]
    #[case::a_blank_reason(&[(".env", " ")], "cloned web-shop on vps · not copied: .env (vps gave no reason)")]
    #[case::a_control_reason(&[(".env", "\u{7}\u{200D}")], "cloned web-shop on vps · not copied: .env (vps gave no reason)")]
    fn names_the_project_the_machine_and_only_the_files_it_can_name(#[case] left: &[(&str, &str)], #[case] said: &str) {
        let left: Vec<Unwritten> = left.iter().map(|(path, why)| unwritten(path, why)).collect();

        assert_eq!(cloned_note("web-shop", "vps", &left), said);
    }
}
