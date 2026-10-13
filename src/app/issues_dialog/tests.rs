use crossterm::event::KeyCode;

use super::view;
use crate::issues::browser::tests::{buttons::reading, people::filtering, press};
use crate::issues::browser::{Browser, Room};
use crate::ui;

const ROOM: Room = Room { list_rows: 22, detail_rows: 26, detail_width: 106 };

mod buttons {
    use super::*;

    #[test]
    fn a_notice_shows_until_the_next_key() {
        let mut b = reading();
        b.notice = Some("copied".into());
        let hint = |b: &Browser| match view(b, ROOM, 0) {
            ui::Overlay::Issues(v) => v.hint,
            _ => String::new(),
        };
        let before = hint(&b);
        press(&mut b, KeyCode::Down);
        assert_eq!((before.as_str(), hint(&b) == "copied"), ("copied", false));
    }
}

mod people {
    use super::*;

    #[test]
    fn each_source_has_its_two_fields() {
        let b = filtering();
        let ui::Overlay::Issues(view) = view(&b, ROOM, 0) else { panic!("not the issues") };
        let ui::IssuesBody::List { items, .. } = view.body else { panic!("not a list") };
        let rows: Vec<String> = items.iter().map(|i| format!("{} {}", i.key, i.title)).collect();
        assert_eq!(
            rows,
            ["GitHub assignee anyone", "GitHub author anyone", "Shortcut owner anyone", "Shortcut requester anyone"]
        );
    }
}
