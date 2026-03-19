# alchemy-os

Run locally:
```shell
cargo run -p alchemy-loader --target x86_64-unknown-uefi 
```

## Todo List

- add a dedicated LMAX Disruptor ring buffer for logging to get rid of the mutex writer
- build alchemy paging layer
- build real timer interrupt
- upgrade scheduler from cooperative to preemptive
- make actor memory ownership cleaner
- better fault supervision
