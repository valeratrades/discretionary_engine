## Assumptions
- strictly one asset per `Position`. No methods for acquiring several assets at once will ever be introduced.

### Current (may change in the future)
- no two `Position`s are opened on the same symbol

- no new `Position`s on account are opened outside of the engine

- orders are placed immediately (not that far off, as most of the time we will spam the thing until it accepts, and only other action that will need to be taken is to prevent any increases in exposure while we have any mismatches).
