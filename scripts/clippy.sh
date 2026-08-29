#!/usr/bin/env bash
# Workspace clippy gate. rustc 1.98 style nits on the imported tree are
# allowed on the CLI so `-D warnings` still fails on correctness lints.
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo clippy --workspace --all-targets "$@" -- \
  -D warnings \
  -A clippy::collapsible_if \
  -A clippy::collapsible_match \
  -A clippy::for_kv_map \
  -A clippy::explicit_counter_loop \
  -A clippy::useless_conversion \
  -A clippy::useless_borrows_in_formatting \
  -A clippy::chunks_exact_to_as_chunks \
  -A clippy::manual_midpoint \
  -A clippy::unnecessary_map_or \
  -A clippy::redundant_clone \
  -A clippy::cloned_instead_of_copied \
  -A clippy::needless_pass_by_value \
  -A clippy::too_many_arguments \
  -A clippy::type_complexity \
  -A clippy::large_enum_variant \
  -A clippy::result_large_err \
  -A clippy::assigning_clones \
  -A clippy::field_reassign_with_default \
  -A clippy::struct_excessive_bools \
  -A clippy::fn_params_excessive_bools \
  -A clippy::manual_clamp \
  -A clippy::manual_range_contains \
  -A clippy::implicit_clone \
  -A clippy::uninlined_format_args \
  -A clippy::format_push_string \
  -A clippy::doc_markdown \
  -A clippy::missing_errors_doc \
  -A clippy::missing_panics_doc \
  -A clippy::must_use_candidate \
  -A clippy::cast_possible_truncation \
  -A clippy::cast_sign_loss \
  -A clippy::cast_possible_wrap \
  -A clippy::cast_precision_loss \
  -A clippy::cast_lossless \
  -A clippy::expect_used \
  -A clippy::unwrap_used \
  -A clippy::print_stdout \
  -A clippy::print_stderr \
  -A clippy::unnecessary_sort_by \
  -A clippy::iter_without_into_iter \
  -A clippy::module_inception \
  -A clippy::derivable_impls \
  -A unknown_lints
