# Immediate steps
1. proc macro workshop <https://github.com/dtolnay/proc-macro-workshop> in ~/s/l/proc_macros
1. proc macro for name conversion. With it, make definitive string deserialization.
1. derive macro to deserialize ProtocolsSpec from Vec<String>

1. restructure to work with only one for now
1. create enum Position
1. skeleton of `execute` on it



# Features
- [ ] dynamically pull max_order_size and max_leverage for all futures pairs // can this be done via a websocket?

- [ ] automatically adjust leverage to not borrow when it is possible to use your own money
