## NATS Client Protocol using Tokio's `codec::Framed{Read, Write}` and `codec::{Encoder, Decoder}`

- `/nats_codec`: NATS Command definitions + `Decoder` and `Encoder` to work with asynchronous I/O in a type safe manner
- `/nats_sans_io`: 100% synchronous state-machine driven implementation of the NATS protocol for clients. For more information on sans-IO, see [this blog post](https://www.firezone.dev/blog/sans-io).
- `/nats_client`: An asynchronous NATS client in Tokio that uses `/nats_sans_io` to facilitate communication, supporting both subscribing and publishing, with matching examples

## Examples: (NATS has a public server at demo.nats.io:4222)

* Publisher: Publishes the UTC+05:30 timestamp to the specified NATS server

```shell
$ RUST_LOG=info c r --example publisher -- --help                          
Usage: publisher <HOST> <PORT> <SUBJECT> [LIMIT]

Arguments:
  <HOST>     
  <PORT>     
  <SUBJECT>  
  [LIMIT]    

Options:
  -h, --help  Print help
```

* Send 5 messages to `time.us.east` @ `demo.nats.io:4222`

```shell
$ RUST_LOG=info cargo r --example publisher -- demo.nats.io 4222 time.us.east 5
```

* Subscriber: Publishes the UTC+05:30 timestamp to the specified NATS server

```shell
$ RUST_LOG=info c r --example subscriber -- --help
Usage: subscriber <HOST> <PORT> <SUBJECT> [MAX_MSGS] [QUEUE_GROUP]

Arguments:
  <HOST>         
  <PORT>         
  <SUBJECT>      
  [MAX_MSGS]     
  [QUEUE_GROUP]  

Options:
  -h, --help  Print help
```


* Receive 3 messages with the `time.us.>` subject from `demo.nats.io:4222`

```shell
RUST_LOG=info cargo r --example publisher -- demo.nats.io 4222 time.us.> 3
```
