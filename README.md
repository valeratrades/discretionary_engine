# Discretionary Engine
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.82+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/discretionary_engine.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/discretionary_engine)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/discretionary_engine)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/valeratrades/discretionary_engine/ci.yml?branch=master&style=for-the-badge&style=flat-square" height="20">](https://github.com/valeratrades/discretionary_engine/actions?query=branch%3Amaster)
![Lines Of Code](https://img.shields.io/badge/LoC-3096-lightblue)

Places and follows a position from a definition of _what the target position is_

## Usage
Example query:
```sh
discretionary_engine new --size=-0.1 --symbol=ADAUSDT '-f=sar:t5m:s0.07:i0.02:m0.15' '-f=tpsl:t0.4884:s0.5190'
```
This would open a new position on ADA, where:
- Side: SELL, as the provided size is negative
- Size: 10% of the total balance
- rm_protocol_1: sar indicator, following the price action on 5m timeframe, with starting value 0.07, increase of 0.02, max 0.15
- rm_protocol_2: static tp and sl, which are set at 0.4884 and 0.5190, respectively

## Coverage
Currently only working with Binance.

## Configuration
Config is read from ${HOME}/.config/discretionary_engine.toml by default, but can also be specified via `--config` cli argument.

An example config can be found in ./examples/config.toml

## Assumptions
- strictly one asset per `Position`. No methods for acquiring several assets at once will ever be introduced.

### Current (may change in the future)
- no two `Position`s are opened on the same symbol

- no new `Position`s on account are opened outside of the engine

- orders are placed immediately (not that far off, as most of the time we will spam the thing until it accepts, and only other action that will need to be taken is to prevent any increases in exposure while we have any mismatches).

<br>

<sup>
This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a>.
</sup>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>

