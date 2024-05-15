# Architecture
```mermaid
flowchart TD
    Hub["Hub"]

    subgraph positions ["POSITIONS"]
        subgraph cluster_position_1 ["Position I"]
            Protocol1_Params1 --> S
            Protocol3_Params1 --> S

            F --> |"apply fill mask on ProtocolOrders
            objects protocols are sending,
            and refresh current suggested
            orders on Position"| S

            S["All suggested orders for this Position"]

            F["Fill port of the Position"]
        end
        PositionII["Position II"]
        PositionIII["Position III"]
    end
    PositionII --> Hub
    PositionIII --> Hub
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
    Hub -->|"fill"| F


    subgraph cluster_exchanges ["Exchange API modules"]
        BinanceFutures
        BinanceSpot
        BybitFutures
        Coinbase
    end
  
    BinanceFutures -->|"fill"| Hub
    BinanceSpot -->|"fill"| Hub
    BybitFutures -->|"fill"| Hub
    Coinbase -->|"fill"| Hub
    Hub --> BinanceFutures
    Hub --> BinanceSpot
    Hub --> BybitFutures
    Hub --> Coinbase
```

TODO: https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html


