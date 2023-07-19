# llq

[![Cargo](https://img.shields.io/crates/v/llq.svg)](https://crates.io/crates/llq)
[![Documentation](https://docs.rs/llq/badge.svg)](https://docs.rs/llq)

A wait-free single-producer single-consumer linked-list queue with individually reusable nodes.

Queue operations do not block or allocate memory. Individual nodes are allocated and managed separately, and can be reused on multiple queues.

## Examples

Using a queue to send values between threads:

```rust
use llq::{Node, Queue};

let (mut producer, mut consumer) = Queue::<usize>::new().split();

producer.push(Node::new(0));
producer.push(Node::new(1));
producer.push(Node::new(2));

std::thread::spawn(move || {
    assert_eq!(*consumer.pop().unwrap(), 0);
    assert_eq!(*consumer.pop().unwrap(), 1);
    assert_eq!(*consumer.pop().unwrap(), 2);
    assert!(consumer.pop().is_none());
}).join().unwrap();

```

Reusing a node between multiple queues:

```rust
use llq::{Node, Queue};

let (mut producer1, mut consumer1) = Queue::<usize>::new().split();
let (mut producer2, mut consumer2) = Queue::<usize>::new().split();

let node = Node::new(3);
producer1.push(node);
let node = consumer1.pop().unwrap();
producer2.push(node);
let node = consumer2.pop().unwrap();

assert_eq!(*node, 3);
```

## License

`llq` is distributed under the terms of both the [MIT license](LICENSE-MIT) and the [Apache license, version 2.0](LICENSE-APACHE). Contributions are accepted under the same terms.
