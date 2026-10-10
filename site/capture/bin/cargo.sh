#!/bin/bash
g=$'\033[1;32m'; r=$'\033[0m'; red=$'\033[31m'
printf '%s   Compiling%s web-shop v0.5.0\n' "$g" "$r"
sleep 0.2
printf '%s    Finished%s test profile in 2.41s\n' "$g" "$r"
printf '%s     Running%s unittests src/main.rs\n\n' "$g" "$r"
printf 'running 7 tests\n'
for t in checkout::totals_include_tax checkout::coupons_stack returns::label_is_printed theme::dark_background_is_dark theme::light_is_the_default returns::postcode_is_trimmed; do
  printf 'test %s ... \033[32mok\033[0m\n' "$t"
done
printf 'test returns::empty_address ... %sFAILED%s\n\n' "$red" "$r"
printf 'failures:\n\n---- returns::empty_address stdout ----\n'
printf 'panicked at src/returns/address.rs:12:33:\ncalled `Option::unwrap()` on a `None` value\n\n'
printf 'test result: %sFAILED%s. 6 passed; 1 failed\n' "$red" "$r"
exit 101
