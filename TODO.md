- [ ] implement trailing stop on followup (hardcode everything for now)
    - [x] test that we're printing correct orders from within attach
    - [ ] define target order and execute them

Also, probably possible to centralize the `FollowupProtocol` and `AcquisitionProtocol` under one umbrella, then have some tag or enum to distinguish.

- [ ] transition acquisition to follow the protocols standard

- [ ] dynamically pull max_order_size and max_leverage for all futures pairs

- [ ] automatically adjust leverage to absolute minimum; to diminish carry costs
