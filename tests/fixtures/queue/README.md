# queue.json fixtures

The exact text `persist::save` wrote before bots (0.13.83): `v1.json` has one-off entries only, `v2.json` a routine, its runs and the `removed` and `kept` states. A change that alters how a file without a bot entry is written fails `queue::tests::bot_entries`; a new saved format adds a new fixture instead of changing these.
