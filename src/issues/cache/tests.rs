use super::*;
use crate::issues::issue;
use crate::test_util::TempDir;

fn key(source: Source) -> Key {
    Key { source, project: (source == Source::Github).then(|| "/p".into()), query: Query::default() }
}

#[test]
fn a_list_survives_a_new_cache_on_the_same_file() {
    let tmp = TempDir::new();
    let path = tmp.path().join("issues.json");
    Cache::new(Some(path.clone())).put(key(Source::Github), vec![issue(Source::Github, 7, "x")]);

    let found = Cache::new(Some(path)).get(&key(Source::Github));

    assert_eq!(found.map(|l| l.len()), Some(1));
}

#[test]
fn another_query_is_another_list() {
    let mut cache = Cache::new(None);
    cache.put(key(Source::Github), vec![issue(Source::Github, 7, "x")]);
    let closed = Key { query: Query { closed: true, ..Query::default() }, ..key(Source::Github) };
    assert_eq!(cache.get(&closed), None);
}

#[test]
fn forgetting_a_source_drops_its_lists_only() {
    let mut cache = Cache::new(None);
    cache.put(key(Source::Github), Vec::new());
    cache.put(key(Source::Linear), Vec::new());

    cache.forget(Source::Linear);

    assert_eq!((cache.get(&key(Source::Github)).is_some(), cache.get(&key(Source::Linear))), (true, None));
}

#[test]
fn keeps_only_the_most_recent_lists() {
    let mut cache = Cache::new(None);
    for n in 0..=MAX_LISTS {
        let project = Some(PathBuf::from(format!("/p{n}")));
        cache.put(Key { project, ..key(Source::Github) }, Vec::new());
    }
    let oldest = Key { project: Some("/p0".into()), ..key(Source::Github) };
    assert_eq!((cache.lists.len(), cache.get(&oldest)), (MAX_LISTS, None));
}

#[test]
fn a_corrupt_file_is_an_empty_cache() {
    let tmp = TempDir::new();
    let path = tmp.path().join("issues.json");
    std::fs::write(&path, "{ nope").expect("write");
    assert_eq!(Cache::new(Some(path)).get(&key(Source::Github)), None);
}
