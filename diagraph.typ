#import "@preview/diagraph:0.2.1": *

#raw-render(
  ```
  digraph {
		subgraph cluster_position_1 {
			label = "Position I"
			Protocol1_Params1 -> S
			Protocol3_Params1 -> S
			S -> Hub [label = "Knowing how much
	each protocol manages,
	convert suggested orders,
	(size as % of total under
	protocol's management),
	into notional sizes.
	After choose up to
	target position size
	from them, so as to not
	risk having additional
	stale exposure"]
			F -> S [label="apply fill mask on ProtocolOrders
	objects protocols are sending,
	and refresh current suggested
	orders on Position"]

			S [label = "All suggested orders for this Position"]
			F [label = "Fill port of the Position"]
		}

		"Position II" -> Hub [dir=both]
		"Position III" -> Hub [dir=both]
		Hub -> F [label="fill"]

		Hub -> BinanceFutures
		BinanceFutures -> Hub [label="fill"]
		Hub -> BinanceSpot
		BinanceSpot -> Hub [label = "fill"]
		Hub -> BybitFutures
		BybitFutures -> Hub [label = "fill"]
		Hub -> Coinbase
		Coinbase -> Hub [label = "fill"]
  }
  ```,
)
