# Discretionary Engine
```mermaid
flowchart TD
    Protocol1 --> |Position| U
    Protocol2 --> |Position| U
    Protocol3 --> |Position| U
    U[Recieve and insert updated request to HashMap<Protocol, Vec<ConceptualOrder>>] --> |select! in Position| R
    
    F[Recieved a notif of a fill back from the hub, //includes a random uuid and we can't resend new target without the uuid of the latest fill] --> |select! in Position| R

    R(Translate all requested orders from percents to nominal sizing, and choose up to the max size of the position from closest to current price orders) --> |Hub| Hub
    Hub(Receives updates on currently desired Vec<ConceptualOrder> from all Positions. If Position sent correct uuid of the last fill on it, we recalculate the target HashMap<Exchange, Order>, based on knowledge of the sizes,sides,symbol of each ConceptualOrder and liquidity,fees,E-of-latency,cost-of-moving-money-between-exchanges, etc., on each exchange)
    Hub --> |update orders| A[BinanceFutures]
    Hub --> |update orders| B[BinanceSpot]
    Hub --> |update orders| C[BybitFutures]
    Hub --> |update orders| D[Coinbase]
```

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

## Current assumptions
- no two positions are opened on the same symbol

- no new positions on account are opened outside of the engine

- orders are placed immediately (not that far off, as most of the time we will spam the thing until it accepts, and only other action that will need to be taken is to prevent any increases in exposure while we have any mismatches).

# TODO for next version

- [x] is there a pattern to connect members of two enums?
    If yes, implement it, otherwise make them HashMap<str, String>

- [x] impl all `ProtocolCache` for Followups

- [ ] impl all `FollowupProtocol` for Followups

... To make price requests sync, and not have to deal with async traits, I'm making a compound orderbook implementation as a separate project now...


## State
Currently I am rewriting handling and initialization of exchange interactions in a separate crate.
Once I have a reasonable core here, it will be possible to either integrate it into here, or the other way around.
