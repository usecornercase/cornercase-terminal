use rstest::rstest;

use super::*;

const LINUX: &str = "MemTotal:       32768000 kB\nMemFree:         1024000 kB\nMemAvailable:    9437184 kB\n\
                     Buffers:          204800 kB\nCached:          8192000 kB\n";
const OLD_KERNEL: &str = "MemTotal:        2048000 kB\nMemFree:          512000 kB\nBuffers:          102400 kB\n\
                          Cached:           307200 kB\n";

#[rstest]
#[case::available_from_its_own_line(LINUX, Some(Memory { total: 32_768_000 * 1024, available: 9_437_184 * 1024 }))]
#[case::free_buffers_and_cache_on_an_old_kernel(
    OLD_KERNEL,
    Some(Memory { total: 2_048_000 * 1024, available: (512_000 + 102_400 + 307_200) * 1024 })
)]
#[case::no_total("MemFree: 10 kB\nMemAvailable: 5 kB\n", None)]
#[case::a_unit_it_does_not_know("MemTotal: 10 MB\nMemAvailable: 5 kB\n", None)]
#[case::a_zero_total("MemTotal: 0 kB\nMemAvailable: 0 kB\n", None)]
#[case::more_available_than_total("MemTotal: 10 kB\nMemAvailable: 50 kB\n", Some(Memory { total: 10_240, available: 10_240 }))]
#[case::a_field_that_only_starts_alike("MemTotalish: 99 kB\nMemTotal: 4 kB\nMemAvailable: 1 kB\n", Some(Memory { total: 4096, available: 1024 }))]
fn meminfo_reads_total_and_available(#[case] text: &str, #[case] read: Option<Memory>) {
    assert_eq!(meminfo(text), read);
}

#[rstest]
#[case::free_inactive_and_purgeable(1 << 34, [1000, 2000, 500], 16_384, Some(Memory { total: 1 << 34, available: 3500 * 16_384 }))]
#[case::never_above_the_total(1000, [10, 10, 10], 4096, Some(Memory { total: 1000, available: 1000 }))]
#[case::no_page_size(1 << 30, [1, 1, 1], 0, None)]
#[case::no_total(0, [1, 1, 1], 4096, None)]
fn pages_add_up_to_what_is_available(
    #[case] total: u64,
    #[case] pages: [u64; 3],
    #[case] page: u64,
    #[case] read: Option<Memory>,
) {
    assert_eq!(from_pages(total, pages, page), read);
}

#[test]
fn this_computer_has_some_memory_available() {
    let memory = read().expect("the system's memory");

    assert!(memory.available > 0, "{memory:?}");
    assert!(memory.total >= memory.available, "{memory:?}");
}
