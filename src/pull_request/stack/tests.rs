use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_gh, fixture};

const STACK: &str = fixture!("gh/2.101.0/stack.json");
const NONE: &str = fixture!("gh/2.101.0/stack-none.json");
const UNSUPPORTED: &str = fixture!("gh/2.101.0/stack-unsupported.json");
const UNSUPPORTED_ERROR: &str = fixture!("gh/2.101.0/stack-unsupported.stderr");

const URL: &str = "https://github.com/acme/web-shop/pull/483";

fn printing(tmp: &TempDir, json: &str) -> std::path::PathBuf {
    fake_gh(tmp.path(), &format!("echo \"$@\" > args; cat <<'EOF'\n{json}\nEOF"))
}

fn layer(position: usize, number: u64) -> Layer {
    Layer {
        position,
        number,
        title: format!("layer {number}"),
        url: format!("https://github.com/acme/web-shop/pull/{number}"),
        state: State::Open,
        checks: Some(Ci::Passing),
        conflicting: Some(false),
        review: None,
    }
}

fn stack_of(layers: Vec<Layer>) -> Stack {
    Stack { number: 480, base: "main".into(), size: layers.len(), layers }
}

mod read {
    use super::*;

    fn read_in(tmp: &TempDir, gh: &Path, url: &str) -> Result<Answer> {
        read(gh, tmp.path(), url)
    }

    #[test]
    fn sends_the_query_for_the_url_to_its_host() {
        let tmp = TempDir::new();

        read_in(&tmp, &printing(&tmp, STACK), URL).expect("read");

        let args = std::fs::read_to_string(tmp.path().join("args")).expect("args");
        assert_eq!(args.trim(), format!("api graphql --hostname github.com -f query={QUERY} -f url={URL}"));
    }

    #[test]
    fn reads_a_stack_bottom_first() {
        let tmp = TempDir::new();

        let Answer::In(stack) = read_in(&tmp, &printing(&tmp, STACK), URL).expect("read") else { panic!("no stack") };

        assert_eq!((stack.number, stack.base.as_str(), stack.size), (484, "main", 3));
        let numbers: Vec<(usize, u64)> = stack.layers.iter().map(|l| (l.position, l.number)).collect();
        assert_eq!(numbers, [(1, 482), (2, 483), (3, 485)]);
        let first = &stack.layers[0];
        assert_eq!(first.title, "Returns page crashes on an empty address");
        assert_eq!(first.url, "https://github.com/acme/web-shop/pull/482");
        assert_eq!((first.state, first.checks, first.conflicting), (State::Open, Some(Ci::Failing), Some(false)));
        assert_eq!(first.review, Some(Review::Required));
        let last = &stack.layers[2];
        assert_eq!((last.state, last.checks, last.review), (State::Draft, Some(Ci::Pending), None));
        assert_eq!(stack.layers[1].conflicting, None);
    }

    #[test]
    fn layers_are_sorted_by_position() {
        let tmp = TempDir::new();
        let json = r#"{"data":{"resource":{"stack":{"number":1,"size":2,"baseRefName":"main","entries":{"nodes":[
            {"position":2,"pullRequest":{"number":8,"state":"OPEN"}},
            {"position":1,"pullRequest":{"number":7,"state":"MERGED"}}]}}}}}"#;

        let Answer::In(stack) = read_in(&tmp, &printing(&tmp, json), URL).expect("read") else { panic!("no stack") };

        assert_eq!(stack.layers.iter().map(|l| l.number).collect::<Vec<_>>(), [7, 8]);
    }

    #[test]
    fn a_pull_request_in_no_stack_is_alone() {
        let tmp = TempDir::new();

        assert_eq!(read_in(&tmp, &printing(&tmp, NONE), URL).expect("read"), Answer::Alone);
    }

    #[test]
    fn a_url_that_is_no_pull_request_is_alone() {
        let tmp = TempDir::new();
        let gh = printing(&tmp, r#"{"data":{"resource":null}}"#);

        assert_eq!(read_in(&tmp, &gh, URL).expect("read"), Answer::Alone);
    }

    #[test]
    fn a_url_without_a_host_asks_nothing() {
        let tmp = TempDir::new();

        assert_eq!(read_in(&tmp, &printing(&tmp, STACK), "").expect("read"), Answer::Alone);
        assert!(!tmp.path().join("args").exists());
    }

    #[test]
    fn a_host_without_stacks_is_unsupported() {
        let tmp = TempDir::new();
        let script = format!("cat <<'EOF'\n{UNSUPPORTED}\nEOF\ncat >&2 <<'EOF'\n{UNSUPPORTED_ERROR}\nEOF\nexit 1");
        let gh = fake_gh(tmp.path(), &script);

        assert_eq!(read_in(&tmp, &gh, URL).expect("read"), Answer::Unsupported);
    }

    #[test]
    fn other_failures_are_errors() {
        let tmp = TempDir::new();
        let gh = fake_gh(tmp.path(), "echo 'HTTP 502: Bad Gateway' >&2; exit 1");

        let error = read_in(&tmp, &gh, URL).expect_err("gh failed");

        assert_eq!(error.to_string(), "HTTP 502: Bad Gateway");
    }

    #[test]
    fn an_enterprise_url_asks_its_own_host() {
        let tmp = TempDir::new();

        read_in(&tmp, &printing(&tmp, NONE), "https://Git.Acme.Example/acme/web-shop/pull/4").expect("read");

        let args = std::fs::read_to_string(tmp.path().join("args")).expect("args");
        assert!(args.starts_with("api graphql --hostname git.acme.example "), "{args}");
    }
}

mod mapping {
    use super::*;

    fn raw(json: &str) -> Layer {
        let raw: RawPull = serde_json::from_str(json).expect("a pull request");
        Entry { position: 1, pull_request: Some(raw) }.layer().expect("a layer")
    }

    #[rstest]
    #[case::failure("FAILURE", Some(Ci::Failing))]
    #[case::error("ERROR", Some(Ci::Failing))]
    #[case::pending("PENDING", Some(Ci::Pending))]
    #[case::expected("EXPECTED", Some(Ci::Pending))]
    #[case::success("SUCCESS", Some(Ci::Passing))]
    #[case::unknown("SOMETHING", None)]
    fn the_rollup_state_is_the_checks(#[case] state: &str, #[case] want: Option<Ci>) {
        let json = format!(r#"{{"state":"OPEN","statusCheckRollup":{{"state":"{state}"}}}}"#);

        assert_eq!(raw(&json).checks, want);
    }

    #[test]
    fn no_rollup_is_no_checks() {
        assert_eq!(raw(r#"{"state":"OPEN","statusCheckRollup":null}"#).checks, None);
    }

    #[rstest]
    #[case::open("OPEN", false, State::Open)]
    #[case::draft("OPEN", true, State::Draft)]
    #[case::merged("MERGED", false, State::Merged)]
    #[case::closed("CLOSED", true, State::Closed)]
    fn the_state_is_the_pull_requests(#[case] state: &str, #[case] draft: bool, #[case] want: State) {
        assert_eq!(raw(&format!(r#"{{"state":"{state}","isDraft":{draft}}}"#)).state, want);
    }

    #[rstest]
    #[case::approved("\"APPROVED\"", Some(Review::Approved))]
    #[case::changes("\"CHANGES_REQUESTED\"", Some(Review::ChangesRequested))]
    #[case::required("\"REVIEW_REQUIRED\"", Some(Review::Required))]
    #[case::none("null", None)]
    fn the_review_decision(#[case] decision: &str, #[case] want: Option<Review>) {
        assert_eq!(raw(&format!(r#"{{"state":"OPEN","reviewDecision":{decision}}}"#)).review, want);
    }

    #[rstest]
    #[case::conflicting("CONFLICTING", Some(true))]
    #[case::mergeable("MERGEABLE", Some(false))]
    #[case::unknown("UNKNOWN", None)]
    fn mergeable_is_conflicting(#[case] mergeable: &str, #[case] want: Option<bool>) {
        assert_eq!(raw(&format!(r#"{{"state":"OPEN","mergeable":"{mergeable}"}}"#)).conflicting, want);
    }

    #[rstest]
    #[case("https://github.com/a/b/pull/1", Some("github.com"))]
    #[case("http://Git.Example/a/b/pull/1", Some("git.example"))]
    #[case("git@github.com:a/b", None)]
    #[case("", None)]
    fn the_host_of_a_url(#[case] url: &str, #[case] want: Option<&str>) {
        assert_eq!(host(url).as_deref(), want);
    }
}

mod blocker {
    use super::*;

    fn failing(mut layer: Layer) -> Layer {
        layer.checks = Some(Ci::Failing);
        layer
    }

    fn conflicting(mut layer: Layer) -> Layer {
        layer.conflicting = Some(true);
        layer
    }

    fn changes(mut layer: Layer) -> Layer {
        layer.review = Some(Review::ChangesRequested);
        layer
    }

    fn who(stack: &Stack, number: u64) -> Option<(u64, Block)> {
        stack.blocker(number).map(|(layer, block)| (layer.number, block))
    }

    #[test]
    fn a_failing_layer_below_blocks_on_checks() {
        let stack = stack_of(vec![failing(layer(1, 1)), layer(2, 2)]);

        assert_eq!(who(&stack, 2), Some((1, Block::Checks)));
    }

    #[test]
    fn a_conflicting_layer_below_blocks_on_conflicts() {
        let stack = stack_of(vec![conflicting(layer(1, 1)), layer(2, 2)]);

        assert_eq!(who(&stack, 2), Some((1, Block::Conflicts)));
    }

    #[test]
    fn on_one_layer_a_conflict_beats_failing_checks() {
        let stack = stack_of(vec![failing(conflicting(layer(1, 1))), layer(2, 2)]);

        assert_eq!(who(&stack, 2), Some((1, Block::Conflicts)));
    }

    #[test]
    fn a_merged_layer_never_blocks() {
        let mut merged = conflicting(layer(1, 1));
        merged.state = State::Merged;
        let stack = stack_of(vec![merged, layer(2, 2)]);

        assert_eq!(who(&stack, 2), None);
    }

    #[test]
    fn changes_requested_block_only_when_nothing_below_conflicts_or_fails() {
        let only = stack_of(vec![changes(layer(1, 1)), layer(2, 2)]);
        let worse = stack_of(vec![changes(layer(1, 1)), failing(layer(2, 2)), layer(3, 3)]);

        assert_eq!(who(&only, 2), Some((1, Block::Review)));
        assert_eq!(who(&worse, 3), Some((2, Block::Checks)));
    }

    #[test]
    fn layers_above_never_count_and_the_lowest_wins() {
        let stack = stack_of(vec![layer(1, 1), failing(layer(2, 2)), failing(layer(3, 3)), failing(layer(4, 4))]);

        assert_eq!(who(&stack, 4), Some((2, Block::Checks)));
        assert_eq!(who(&stack, 2), None);
        assert_eq!(who(&stack, 1), None);
    }
}

mod mark {
    use super::*;

    #[test]
    fn has_the_place_the_size_and_what_blocks() {
        let mut bottom = layer(1, 1);
        bottom.checks = Some(Ci::Failing);
        let stack = stack_of(vec![bottom, layer(2, 2), layer(3, 3)]);

        let mark = stack.mark(2, true).expect("listed");

        assert_eq!((mark.position, mark.size, mark.blocked), (2, 3, Some(Block::Checks)));
        assert_eq!(stack.mark(1, true).expect("listed").blocked, None);
    }

    #[test]
    fn a_pull_request_that_is_not_listed_has_none() {
        assert_eq!(stack_of(vec![layer(1, 1)]).mark(9, true), None);
    }

    #[test]
    fn a_pull_request_that_is_settled_is_never_blocked() {
        let mut bottom = layer(1, 1);
        bottom.checks = Some(Ci::Failing);
        let stack = stack_of(vec![bottom, layer(2, 2)]);

        assert_eq!(stack.mark(2, false).expect("listed").blocked, None);
    }
}

mod keep_known {
    use super::*;

    #[test]
    fn an_unknown_layer_keeps_a_known_conflict() {
        let mut before = stack_of(vec![layer(1, 1), layer(2, 2)]);
        before.layers[0].conflicting = Some(true);
        let mut now = stack_of(vec![layer(1, 1), layer(2, 2)]);
        now.layers[0].conflicting = None;

        now.keep_known(&before);

        assert_eq!(now.layers[0].conflicting, Some(true));
        assert_eq!(now.layers[1].conflicting, Some(false));
    }

    #[test]
    fn another_stack_keeps_nothing() {
        let mut before = stack_of(vec![layer(1, 1)]);
        before.layers[0].conflicting = Some(true);
        let mut now = stack_of(vec![layer(1, 1)]);
        now.number = 481;
        now.layers[0].conflicting = None;

        now.keep_known(&before);

        assert_eq!(now.layers[0].conflicting, None);
    }
}
