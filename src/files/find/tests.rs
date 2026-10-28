use super::*;

fn lines(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| (*t).to_string()).collect()
}

fn at(find: &Find) -> Option<(usize, u32)> {
    find.current().map(|(i, o)| (i, o.line))
}

fn typed(text: &str, lines: &[String], top: (u32, u32)) -> Find {
    let mut find = Find::default();
    find.edit(|q| q.push_str(text), lines, top);
    find
}

fn spread() -> Vec<String> {
    let mut rows = vec![String::new(); 10];
    for n in [1, 4, 8] {
        rows[n] = "a needle".into();
    }
    rows
}

mod matching {
    use super::*;

    fn spans(rows: &[&str], query: &str) -> Vec<(u32, u32, u32)> {
        occurrences(&lines(rows), query).iter().map(|o| (o.line, o.start, o.end)).collect()
    }

    #[test]
    fn lower_case_finds_any_case_and_upper_case_only_itself() {
        assert_eq!(spans(&["Run run RUN"], "run"), [(1, 0, 3), (1, 4, 7), (1, 8, 11)]);
        assert_eq!(spans(&["Run run RUN"], "Run"), [(1, 0, 3)]);
    }

    #[test]
    fn the_query_is_a_literal() {
        assert_eq!(spans(&["abc"], "a.c").len(), 0);
        assert_eq!(spans(&["f(x[0])"], "(x["), [(1, 1, 4)]);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        assert_eq!(spans(&["ñandú café"], "café"), [(1, 6, 10)]);
        assert_eq!(spans(&["é é"], "é"), [(1, 0, 1), (1, 2, 3)]);
    }

    #[test]
    fn a_blank_query_finds_nothing() {
        let find = typed("  ", &lines(&["a  b"]), (1, 0));
        assert_eq!((find.count(), find.current()), (0, None));
    }

    #[test]
    fn every_line_is_searched_in_order() {
        assert_eq!(spans(&["no", "an a", "a"], "a"), [(2, 0, 1), (2, 3, 4), (3, 0, 1)]);
    }
}

mod moving {
    use super::*;

    #[test]
    fn typing_goes_to_the_first_occurrence_from_the_top_line() {
        assert_eq!(at(&typed("needle", &spread(), (4, 0))), Some((1, 5)));
        assert_eq!(at(&typed("needle", &spread(), (1, 0))), Some((0, 2)));
    }

    #[test]
    fn typing_below_the_last_occurrence_wraps_to_the_first() {
        assert_eq!(at(&typed("needle", &spread(), (10, 0))), Some((0, 2)));
    }

    #[test]
    fn steps_wrap_at_both_ends() {
        let mut find = typed("needle", &spread(), (1, 0));
        find.step(-1);
        assert_eq!(at(&find), Some((2, 9)));
        find.step(1);
        assert_eq!(at(&find), Some((0, 2)));
        find.step(1);
        find.step(1);
        assert_eq!(at(&find), Some((2, 9)));
    }

    #[test]
    fn stepping_with_nothing_found_does_nothing() {
        let mut find = typed("zzz", &spread(), (1, 0));
        find.step(1);
        assert_eq!(find.current(), None);
    }

    #[test]
    fn the_occurrences_of_a_line_come_in_order() {
        let find = typed("a", &lines(&["a", "a a", "a"]), (1, 0));
        let starts: Vec<u32> = find.on_line(2).iter().map(|o| o.start).collect();
        assert_eq!((starts, find.on_line(1).len(), find.on_line(4).len()), (vec![0, 2], 1, 0));
    }
}

mod rereading {
    use super::*;

    fn rows(hits: &[usize]) -> Vec<String> {
        let mut rows = vec!["x".to_string(); 10];
        for n in hits {
            rows[*n - 1] = "run".into();
        }
        rows
    }

    #[test]
    fn a_reread_keeps_the_current_occurrence_on_the_nearest_line() {
        let mut find = typed("run", &rows(&[2, 4, 6]), (3, 0));
        assert_eq!(at(&find), Some((1, 4)));
        find.recount(&rows(&[2, 6, 8]));
        assert_eq!((find.count(), at(&find)), (3, Some((1, 6))));
    }

    #[test]
    fn a_reread_that_loses_every_match_finds_none() {
        let mut find = typed("run", &rows(&[2]), (1, 0));
        find.recount(&rows(&[]));
        assert_eq!((find.count(), find.current()), (0, None));
    }

    #[test]
    fn a_find_opened_at_a_line_lands_there_once_the_text_comes() {
        let mut find = Find::new("run", 40, true);
        assert_eq!(find.current(), None);
        let mut text = vec!["x".to_string(); 60];
        for n in [10, 40, 50] {
            text[n - 1] = "run".into();
        }
        find.recount(&text);
        assert_eq!((at(&find), find.focused), (Some((1, 40)), true));
    }
}
