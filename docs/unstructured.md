After some manual trading, updating priorities:

- Must have a "button" to quickly tp a part of the position, or add some to it. The way it'll work is I just do `adjust` instead of `new` position, then provide the id of the target. Id will be its numerical index since _last time there were no positions open_, so that they're a) small, b) don't change. This is only really necessary for `market` and `chase` orders, the partial limit tps are better written out in the main position-control file.
	// keep it simple - at most add an additional argument to `chase` for defining the maximum range before cancel, in nominal or as percents (understood by presence of `%` sign in the arg). Also I guess everything should have a `reduce-only` flag

- Must have a "nuke all orders" and "nuke absolutely everything" buttons

- Must have a way of continuous tp/sl adjustment throughout position's lifetime, (that's done by modifying a text file).

- Chase protocol is desired, tp/sl is a must-have (not sure if it should be raised to the status of intrinsic part of the position. So far leaning towards no, as many fundamental or long-short plays don't assume any)
