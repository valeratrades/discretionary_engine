- [ ] implement trailing stop on followup (hardcode everything for now)
    - [x] test that we're printing correct orders from within attach
    - [ ] read orders from do_followup on the Position
I'm assuming I need to make `Protocol` into an actor, then have `do_followup` take `Vec<Protocl>`. Then we define `ProtocolHandle`, which creates Vec of orders and cache, allowing also to query the orders; build one for each protocol in the Vec. Note that after we're only monitoring the percent executed, and shutting down everything once it's full.

Also, probably possible to centralize the `FollowupProtocol` and `AcquisitionProtocol` under one umbrella, then have some tag or enum to distinguish.

- [ ] transition acquisition to follow the protocols standard

- [ ] dynamically pull max_order_size and max_leverage for all futures pairs

- [ ] automatically adjust leverage to absolute minimum; to diminish carry costs
