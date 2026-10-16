# World generation mismatch baseline

Measured counts from `proto_chunk_test` on `core-functionality` `70a0b846e9b101ce717c6e00aaefe681bd590108`. This note does not change generation. The allowances in `verify_chunk_noise`, `verify_chunk_surface`, and `verify_chunk_carvers` are unchanged.

`b0c48b2d7` (`fix: many generation mismatches`) is now on this branch through `core-functionality` `f453f24a`. It deleted the 8x8 grid fixtures and split the suite. The tables below are the `70a0b846e` measurement. They are not the output of the current command. Test 2 in [A-45.md](../playtest/A-45.md) is the check for the current suite.

A later generation change is worse when any count in this note goes up, for a test that still exists. A count that stays under the allowance can still be a regression. Do not raise the allowances to make a new run pass.

## Command

```bash
cargo test -j 2 -p pumpkin-world --lib generation::proto_chunk_test -- --nocapture --test-threads=2
```

Run from the repository root, debug profile, `rustc 1.99.0 (b940084d7 2026-09-28)`.

Result: `ok. 43 passed; 0 failed; 0 ignored; 0 measured; 214 filtered out; finished in 289.81s`.

`count_dump_mismatches` prints `[<name>] Total mismatches: N` only when `N > 0`. A passing test with no such line is 0. The grids below fill those cells with 0.

Each count is the number of block-state ids inside the dumped window that differ from the fixture in `assets/tests/`. An overworld window in this suite is 384 blocks high (98304 states). A nether or end window is 128 blocks high (32768 states). The test also requires air above that window. These fixtures are the checked-in dumps. This run did not generate a new vanilla world and did not read a region file.

## Allowances

Unchanged. One chunk, not the suite total.

| Stage | Function | Allowed mismatches |
| --- | --- | --- |
| Noise | `verify_chunk_noise` | 6000 |
| Surface | `verify_chunk_surface` | 7500 |
| Carvers | `verify_chunk_carvers` | 8000 |

## Suite totals

| Stage | Chunks | Block states compared | Mismatches | Chunks with a mismatch | Worst chunk |
| --- | --- | --- | --- | --- | --- |
| Noise | 76 | 7208960 | 2 | 2 | 1 |
| Surface | 75 | 7110656 | 2846 | 17 | 546 |
| Carvers | 68 | 6553600 | 9122 | 17 | 6640 |

The worst carver chunk is `nether_carvers_no_blend_no_beard_7_4` at 6640, under the 8000 allowance. Noise and surface are far under their allowances. The allowances are not the measured baseline.

## Noise

Seed, dimension, and chunk are the arguments passed to `verify_chunk_noise`. Every single-chunk noise test matched.

| Test | Seed | Dimension | Chunk | Mismatches | States |
| --- | --- | --- | --- | --- | --- |
| `no_blend_no_beard_0_0` | 0 | overworld | 0, 0 | 0 | 98304 |
| `no_blend_no_beard_7_4` | 0 | overworld | 7, 4 | 0 | 98304 |
| `no_blend_no_beard_only_cell_cache_interpolated_0_0` | 0 | overworld | 0, 0 | 0 | 98304 |
| `no_blend_no_beard_badlands_minus595_544` | 0 | overworld | -595, 544 | 0 | 98304 |
| `no_blend_no_beard_frozen_ocean_minus119_183` | 0 | overworld | -119, 183 | 0 | 98304 |
| `no_blend_no_beard_13579_minus6_11` | 13579 | overworld | -6, 11 | 0 | 98304 |
| `no_blend_no_beard_13579_minus2_15` | 13579 | overworld | -2, 15 | 0 | 98304 |
| `no_blend_no_beard_13579_minus7_9` | 13579 | overworld | -7, 9 | 0 | 98304 |
| `nether_noise_no_blend_no_beard_0_0` | 0 | nether | 0, 0 | 0 | 32768 |
| `nether_noise_no_blend_no_beard_7_4` | 0 | nether | 7, 4 | 0 | 32768 |
| `end_noise_no_blend_no_beard_0_0` | 0 | end | 0, 0 | 0 | 32768 |
| `end_noise_no_blend_no_beard_7_4` | 0 | end | 7, 4 | 0 | 32768 |

`grid_8x8_noise` is seed 0, overworld, chunks `x = 0..7`, `z = 0..7`, 98304 states each. Sum 2. Cell is chunk x, row is chunk z.

| z\x | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 7 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

## Surface

| Test | Seed | Dimension | Chunk | Mismatches | States |
| --- | --- | --- | --- | --- | --- |
| `no_blend_no_beard_surface_0_0` | 0 | overworld | 0, 0 | 0 | 98304 |
| `no_blend_no_beard_surface_7_4` | 0 | overworld | 7, 4 | 9 | 98304 |
| `no_blend_no_beard_surface_badlands_minus595_544` | 0 | overworld | -595, 544 | 0 | 98304 |
| `no_blend_no_beard_surface_frozen_ocean_minus119_183` | 0 | overworld | -119, 183 | 7 | 98304 |
| `no_blend_no_beard_surface_13579_minus6_11` | 13579 | overworld | -6, 11 | 320 | 98304 |
| `no_blend_no_beard_surface_13579_minus2_15` | 13579 | overworld | -2, 15 | 0 | 98304 |
| `no_blend_no_beard_surface_13579_minus7_9` | 13579 | overworld | -7, 9 | 155 | 98304 |
| `nether_surface_no_blend_no_beard_0_0` | 0 | nether | 0, 0 | 0 | 32768 |
| `nether_surface_no_blend_no_beard_7_4` | 0 | nether | 7, 4 | 0 | 32768 |
| `end_surface_no_blend_no_beard_0_0` | 0 | end | 0, 0 | 0 | 32768 |
| `end_surface_no_blend_no_beard_7_4` | 0 | end | 7, 4 | 0 | 32768 |

Single-chunk surface sum: 491.

`grid_8x8_surface` is seed 0, overworld, 98304 states each. Sum 2355.

| z\x | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 21 |
| 4 | 253 | 170 | 0 | 0 | 0 | 0 | 0 | 9 |
| 5 | 546 | 181 | 0 | 0 | 0 | 0 | 0 | 1 |
| 6 | 330 | 5 | 0 | 0 | 0 | 0 | 0 | 0 |
| 7 | 407 | 364 | 4 | 64 | 0 | 0 | 0 | 0 |

## Carvers

| Test | Seed | Dimension | Chunk | Mismatches | States |
| --- | --- | --- | --- | --- | --- |
| `no_blend_no_beard_carvers_0_0` | 0 | overworld | 0, 0 | 0 | 98304 |
| `no_blend_no_beard_carvers_7_4` | 0 | overworld | 7, 4 | 9 | 98304 |
| `nether_carvers_no_blend_no_beard_0_0` | 0 | nether | 0, 0 | 182 | 32768 |
| `nether_carvers_no_blend_no_beard_7_4` | 0 | nether | 7, 4 | 6640 | 32768 |

Single-chunk carver sum: 6831.

`grid_8x8_carvers` is seed 0, overworld, 98304 states each. Sum 2291.

| z\x | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 24 |
| 4 | 253 | 170 | 0 | 0 | 0 | 0 | 0 | 9 |
| 5 | 546 | 181 | 0 | 0 | 0 | 0 | 0 | 1 |
| 6 | 328 | 5 | 0 | 0 | 0 | 0 | 0 | 0 |
| 7 | 341 | 364 | 4 | 64 | 0 | 0 | 0 | 0 |

## Same run, not gated

Biome tests and structure-start tests in this module assert exact equality. They passed. They have no mismatch budget.

`verify_grid_features` prints a count and does not fail the test. Those counts are not an allowance. A-46 owns feature work. `ConfiguredFeature::NoOp` is still the stand-in for a feature the generator does not run. This note does not pick a family.

| Test | Seed | Dimension | Chunks | States each | Mismatches | Worst chunk |
| --- | --- | --- | --- | --- | --- | --- |
| `grid_8x8_features` | 0 | overworld | 64 | 98304 | 47135 | 1995 |
| `nether_grid_8x8_features` | 0 | nether | 64 | 32768 | 353478 | 11567 |
| `end_grid_8x8_features` | 0 | end | 64 | 32768 | 0 | 0 |

Overworld features, same x/z layout as the noise grid:

| z\x | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 746 | 96 | 677 | 714 | 169 | 58 | 229 | 52 |
| 1 | 1779 | 851 | 715 | 753 | 205 | 85 | 552 | 711 |
| 2 | 594 | 642 | 1144 | 1160 | 671 | 953 | 863 | 176 |
| 3 | 750 | 796 | 555 | 777 | 879 | 1257 | 811 | 220 |
| 4 | 1613 | 1995 | 893 | 1136 | 1540 | 1551 | 855 | 86 |
| 5 | 1454 | 1035 | 1104 | 1348 | 974 | 919 | 1316 | 140 |
| 6 | 1066 | 698 | 1012 | 572 | 415 | 321 | 405 | 203 |
| 7 | 851 | 1031 | 789 | 234 | 277 | 195 | 256 | 211 |

Nether features:

| z\x | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 1901 | 2397 | 4647 | 8931 | 6064 | 2313 | 2136 | 1591 |
| 1 | 2094 | 3425 | 9646 | 7062 | 6538 | 4959 | 3462 | 4299 |
| 2 | 9283 | 6845 | 8454 | 6688 | 11567 | 7196 | 6031 | 4207 |
| 3 | 3998 | 6346 | 8811 | 6827 | 9592 | 4041 | 5650 | 5064 |
| 4 | 5478 | 8489 | 9203 | 6184 | 7828 | 6051 | 2686 | 7265 |
| 5 | 7016 | 5135 | 6586 | 3652 | 4504 | 5750 | 6858 | 1518 |
| 6 | 1714 | 3011 | 6089 | 3868 | 2402 | 7870 | 9273 | 2959 |
| 7 | 2889 | 3243 | 3643 | 3712 | 2643 | 6434 | 7437 | 10023 |

End features: every cell is 0.

## What this run did not measure

- A fresh 26.3 dedicated-server world. The expected blocks are the fixtures already in `assets/tests/`.
- Chunk import. Core still reads 26.3 palettes as air until `core/A-48-chunk-5023` is merged. This run did not read a region file, so it did not use that branch's reader.
- Which configured-feature family accounts for the feature totals. That choice is A-46.
