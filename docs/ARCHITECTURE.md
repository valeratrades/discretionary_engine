# Discretionary Engine
```mermaid
flowchart TD
    subgraph cluster_position_1 ["Position I"]
        Protocol1_Params1 --> S
        Protocol3_Params1 --> S
        S --> |"Knowing how much
        each protocol manages,
        convert suggested orders,
        (size as % of total under
        protocol's management),
        into notional sizes.
        After choose up to
        target position size
        from them, so as to not
        risk having additional
        stale exposure"| Hub
        F --> |"apply fill mask on ProtocolOrders
        objects protocols are sending,
        and refresh current suggested
        orders on Position"| S
        S["All suggested orders for this Position"]
        F["Fill port of the Position"]
    end

    PositionII["Position II"] --> Hub
    PositionIII["Position III"] --> Hub
    Hub -->|"fill"| F
    Hub --> BinanceFutures
    BinanceFutures -->|"fill"| Hub
    Hub --> BinanceSpot
    BinanceSpot -->|"fill"| Hub
    Hub --> BybitFutures
    BybitFutures -->|"fill"| Hub
    Hub --> Coinbase
    Coinbase -->|"fill"| Hub
```

TODO: https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html
