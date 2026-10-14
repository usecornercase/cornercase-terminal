pub(crate) trait Markup: Sync {
    fn escape(&self, text: &str) -> String;

    fn bold(&self, text: &str) -> String;

    fn pre(&self, text: &str, room: usize) -> String;

    fn room(&self, header: &str) -> usize;

    fn fits(&self, header: &str, part: &str) -> bool;

    fn markdown(&self, source: &str) -> Vec<String>;

    fn lines(&self, lines: &[String]) -> Vec<String>;
}
